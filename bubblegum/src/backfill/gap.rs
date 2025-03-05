use crate::{error::ErrorKind, Rpc};
use anyhow::Result;
use clap::{Args, Parser};
use sea_orm::{DatabaseConnection, DbBackend, FromQueryResult, Statement, Value};
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::str::FromStr;
use tokio::sync::mpsc::Sender;

const GET_SIGNATURES_FOR_ADDRESS_LIMIT: usize = 1000;

/// Below this limit, the gap will be processed using `GAP_LIMIT_RPC` as limit in the RPC call
const GAP_LIMIT: i64 = 10;

#[derive(Debug, Clone, Args)]
pub struct ConfigBackfiller {
    /// Solana RPC URL
    #[arg(long, env)]
    pub solana_rpc_url: String,
}

#[derive(Debug, Parser, Clone)]
pub struct OverfetchArgs {
    /// Signatures limit for the RPC call
    #[arg(long, env, default_value = "20")]
    pub gap_limit_rpc: usize,

    /// Lookup used with LEAD in the SQL query to get the overfetch tx
    #[arg(long, env, default_value = "10")]
    pub overfetch_lookup_limit: i32,
}

const TREE_GAP_SQL: &str = r#"
WITH sequenced_data AS (
    SELECT
        tree,
        seq,
        LEAD(seq) OVER (ORDER BY seq ASC) AS next_seq,
        tx AS current_tx,
        LEAD(tx) OVER (ORDER BY seq ASC) AS next_tx,
        LEAD(tx, $2, tx) OVER (ORDER BY seq ASC) AS overfetch_tx
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
        next_tx AS upper_bound_tx,
        overfetch_tx
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
    upper_bound_tx,
    overfetch_tx
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
    pub overfetch_tx: Vec<u8>,
}

impl TreeGapModel {
    pub async fn find(
        conn: &DatabaseConnection,
        tree: Pubkey,
        overfetch_lookup_limit: i32,
    ) -> Result<Vec<Self>, ErrorKind> {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            TREE_GAP_SQL,
            vec![
                Value::Bytes(Some(Box::new(tree.as_ref().to_vec()))),
                Value::Int(Some(overfetch_lookup_limit)),
            ],
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

        let overfetch = if model.gap_end_seq - model.gap_start_seq < GAP_LIMIT {
            match Signature::try_from(model.overfetch_tx) {
                Ok(sig) => {
                    // Account for overfetch tx falling back to its default value
                    if sig == lower {
                        None
                    } else {
                        Some(sig)
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };

        Ok(Self::new(tree, Some(upper), Some(lower), overfetch))
    }
}

pub struct TreeGapFill {
    tree: Pubkey,
    before: Option<Signature>,
    until: Option<Signature>,
    overfetch_tx: Option<Signature>,
}

impl TreeGapFill {
    pub const fn new(
        tree: Pubkey,
        before: Option<Signature>,
        until: Option<Signature>,
        overfetch_tx: Option<Signature>,
    ) -> Self {
        Self {
            tree,
            before,
            until,
            overfetch_tx,
        }
    }

    pub async fn crawl(
        &self,
        client: Rpc,
        sender: Sender<Signature>,
        overfetch_args: OverfetchArgs,
    ) -> Result<()> {
        let mut before = self.before;

        let (limit, until) = if self.overfetch_tx.is_some() {
            before = self.overfetch_tx;
            (Some(overfetch_args.gap_limit_rpc), None)
        } else {
            (None, self.until)
        };

        loop {
            let sigs = client
                .get_signatures_for_address(&self.tree, before, until, limit)
                .await?;
            let sig_count = sigs.len();

            let successful_transactions = sigs
                .iter()
                .filter(|transaction| transaction.err.is_none())
                .collect::<Vec<&RpcConfirmedTransactionStatusWithSignature>>();

            for sig in successful_transactions.iter() {
                let sig = Signature::from_str(&sig.signature)?;
                sender.send(sig).await?;
            }

            if let Some(last_sig) = sigs.last() {
                before = Some(Signature::from_str(&last_sig.signature)?);
            }

            if sig_count < GET_SIGNATURES_FOR_ADDRESS_LIMIT {
                break;
            }
        }

        Ok(())
    }
}
