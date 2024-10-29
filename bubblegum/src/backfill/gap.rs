use crate::{error::ErrorKind, Rpc};
use anyhow::Result;
use clap::Args;
use sea_orm::{DatabaseConnection, DbBackend, FromQueryResult, Statement, Value};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::str::FromStr;
use tokio::sync::mpsc::Sender;

const GET_SIGNATURES_FOR_ADDRESS_LIMIT: usize = 1000;

#[derive(Debug, Clone, Args)]
pub struct ConfigBackfiller {
    /// Solana RPC URL
    #[arg(long, env)]
    pub solana_rpc_url: String,
}

const TREE_GAP_SQL: &str = r#"
WITH sequenced_data AS (
    SELECT
        tree,
        seq,
        LEAD(seq) OVER (ORDER BY seq ASC) AS next_seq,
        tx AS current_tx,
        LEAD(tx) OVER (ORDER BY seq ASC) AS next_tx
    FROM
        cl_audits_v2
    WHERE
        tree = $1
),
gaps AS (
    SELECT
        tree,
        seq AS gap_start_seq,
        next_seq AS gap_end_seq,
        current_tx AS lower_bound_tx,
        next_tx AS upper_bound_tx
    FROM
        sequenced_data
    WHERE
        next_seq IS NOT NULL AND
        next_seq - seq > 1
)
SELECT
    tree,
    gap_start_seq,
    gap_end_seq,
    lower_bound_tx,
    upper_bound_tx
FROM
    gaps
ORDER BY
    gap_start_seq;
"#;

#[derive(Debug, FromQueryResult, PartialEq, Clone)]
pub struct TreeGapModel {
    pub tree: Vec<u8>,
    pub gap_start_seq: i64,
    pub gap_end_seq: i64,
    pub lower_bound_tx: Vec<u8>,
    pub upper_bound_tx: Vec<u8>,
}

impl TreeGapModel {
    pub async fn find(conn: &DatabaseConnection, tree: Pubkey) -> Result<Vec<Self>, ErrorKind> {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            TREE_GAP_SQL,
            vec![Value::Bytes(Some(Box::new(tree.as_ref().to_vec())))],
        );

        TreeGapModel::find_by_statement(statement)
            .all(conn)
            .await
            .map_err(Into::into)
    }
}

impl TryFrom<TreeGapModel> for TreeGapFill {
    type Error = ErrorKind;

    fn try_from(model: TreeGapModel) -> Result<Self, Self::Error> {
        let tree = Pubkey::try_from(model.tree).map_err(|_| ErrorKind::TryFromPubkey)?;
        let upper =
            Signature::try_from(model.upper_bound_tx).map_err(|_| ErrorKind::TryFromSignature)?;
        let lower =
            Signature::try_from(model.lower_bound_tx).map_err(|_| ErrorKind::TryFromSignature)?;

        Ok(Self::new(tree, Some(upper), Some(lower)))
    }
}

pub struct TreeGapFill {
    tree: Pubkey,
    before: Option<Signature>,
    until: Option<Signature>,
}

impl TreeGapFill {
    pub const fn new(tree: Pubkey, before: Option<Signature>, until: Option<Signature>) -> Self {
        Self {
            tree,
            before,
            until,
        }
    }

    pub async fn crawl(&self, client: Rpc, sender: Sender<Signature>) -> Result<()> {
        let mut before = self.before;

        loop {
            let sigs = client
                .get_signatures_for_address(&self.tree, before, self.until)
                .await?;
            let sig_count = sigs.len();

            let successful_transactions = sigs
                .into_iter()
                .filter(|transaction| transaction.err.is_none())
                .collect::<Vec<RpcConfirmedTransactionStatusWithSignature>>();

            for sig in successful_transactions.iter() {
                let sig = Signature::from_str(&sig.signature)?;

                sender.send(sig).await?;

                before = Some(sig);
            }

            if sig_count < GET_SIGNATURES_FOR_ADDRESS_LIMIT {
                break;
            }
        }

        Ok(())
    }
}
