use std::sync::Arc;

use crate::{
    backfill::gap::{TreeGapFill, TreeGapModel},
    tree::TreeResponse,
    verify::{leaf_proof_result, ProofResult},
    BubblegumContext,
};
use anyhow::Result;
use clap::Parser;
use das_core::{
    create_download_metadata_notifier, DownloadMetadataJsonRetryConfig,
    MetadataJsonDownloadWorkerArgs,
};
use digital_asset_types::{dao::cl_audits_v2, dapi::get_proof_for_asset};
use log::error;
use program_transformers::{ProgramTransformer, TransactionInfo};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, SqlxPostgresConnector};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use tokio::{sync::mpsc::Sender, task::JoinHandle};

use super::{
    FetchedEncodedTransactionWithStatusMeta, GapWorkerArgs, ProgramTransformerWorkerArgs,
    SignatureWorkerArgs,
};

#[derive(Debug, Clone, Parser)]
pub struct TreeWorkerArgs {
    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,

    #[clap(flatten)]
    pub signature_worker: SignatureWorkerArgs,

    #[clap(flatten)]
    pub gap_worker: GapWorkerArgs,

    #[clap(flatten)]
    pub program_transformer_worker: ProgramTransformerWorkerArgs,

    #[clap(long, env, default_value = "false")]
    pub force: bool,
}
impl TreeWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumContext,
        tree: TreeResponse,
        signature_sender: Sender<Signature>,
    ) -> JoinHandle<Result<()>> {
        let db_pool = context.database_pool.clone();
        let gap_worker_args = self.gap_worker.clone();
        let force = self.force;

        tokio::spawn(async move {
            let (gap_worker, tree_gap_sender) = gap_worker_args.start(context, signature_sender)?;

            {
                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(db_pool);

                let mut gaps = TreeGapModel::find(
                    &conn,
                    tree.pubkey,
                    gap_worker_args.overfetch_args.overfetch_lookup_limit,
                )
                .await?
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?;

                let upper_known_seq = if force {
                    None
                } else {
                    cl_audits_v2::Entity::find()
                        .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                        .order_by_desc(cl_audits_v2::Column::Seq)
                        .one(&conn)
                        .await?
                };

                let lower_known_seq = if force {
                    None
                } else {
                    cl_audits_v2::Entity::find()
                        .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                        .order_by_asc(cl_audits_v2::Column::Seq)
                        .one(&conn)
                        .await?
                };

                if let Some(upper_seq) = upper_known_seq {
                    let signature = Signature::try_from(upper_seq.tx.as_ref())?;
                    gaps.push(TreeGapFill::new(tree.pubkey, None, Some(signature), None));
                // Reprocess the entire tree if force is true or if the tree has a seq of 0 to keep the current behavior
                } else if force || tree.seq > 0 {
                    gaps.push(TreeGapFill::new(tree.pubkey, None, None, None));
                }

                if let Some(lower_seq) = lower_known_seq.filter(|seq| seq.seq > 1) {
                    let signature = Signature::try_from(lower_seq.tx.as_ref())?;

                    gaps.push(TreeGapFill::new(tree.pubkey, Some(signature), None, None));
                }

                for gap in gaps {
                    if let Err(e) = tree_gap_sender.send(gap).await {
                        error!("send gap: {:?}", e);
                    }
                }
            }

            drop(tree_gap_sender);

            gap_worker.await?;

            Ok(())
        })
    }
}

#[derive(Debug, Clone, Parser)]
pub struct ProofRepairArgs {
    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,

    #[clap(long, env, default_value = "false")]
    pub repair: bool,
}

impl ProofRepairArgs {
    /// Only start the workers if `--repair` flag is set to true
    pub async fn start(
        &self,
        context: BubblegumContext,
        tree: Pubkey,
    ) -> Result<(Option<JoinHandle<()>>, ProofRepairWorker)> {
        let mut proof_repair_worker = ProofRepairWorker::new(context.clone(), tree);

        if !self.repair {
            return Ok((None, proof_repair_worker));
        }

        let download_config = Arc::new(DownloadMetadataJsonRetryConfig::default());

        let (metadata_json_download_worker, metadata_json_download_sender) = self
            .metadata_json_download_worker
            .start(context.database_pool.clone(), download_config)?;

        let download_metadata_notifier =
            create_download_metadata_notifier(metadata_json_download_sender).await;

        let program_transformer = Arc::new(ProgramTransformer::new(
            context.database_pool.clone(),
            download_metadata_notifier,
        ));

        proof_repair_worker.set_program_transformer(program_transformer);

        Ok((Some(metadata_json_download_worker), proof_repair_worker))
    }
}

#[derive(Clone)]
pub struct ProofRepairWorker {
    context: BubblegumContext,
    tree: Pubkey,
    program_transformer: Option<Arc<ProgramTransformer>>,
}

impl ProofRepairWorker {
    pub const fn new(context: BubblegumContext, tree: Pubkey) -> Self {
        Self {
            context,
            tree,
            program_transformer: None,
        }
    }

    pub fn set_program_transformer(&mut self, program_transformer: Arc<ProgramTransformer>) {
        self.program_transformer = Some(program_transformer);
    }

    /// Query DB for the leaf and if exists a related tx re-fetch from RPC, process transaction
    ///  and re-check proof.
    pub async fn try_repair(
        self,
        proof: Result<ProofResult>,
        leaf_idx: u64,
        asset: Pubkey,
    ) -> Result<ProofResult> {
        match (self.program_transformer, &proof) {
            (Some(program_transformer), Ok(ProofResult::Incorrect) | Ok(ProofResult::NotFound)) => {
                let db = SqlxPostgresConnector::from_sqlx_postgres_pool(self.context.database_pool);

                let tree_pubkey_bytes = self.tree.to_bytes().to_vec();
                let query = cl_audits_v2::Entity::find()
                    .filter(cl_audits_v2::Column::Tree.eq(tree_pubkey_bytes))
                    .filter(cl_audits_v2::Column::LeafIdx.eq(leaf_idx))
                    .order_by_desc(cl_audits_v2::Column::Seq);

                let cl_audits = query.one(&db).await?;
                let cl_audits = match cl_audits {
                    Some(cl_audits) => cl_audits,
                    None => return proof,
                };

                let sig = Signature::try_from(cl_audits.tx.as_ref()).unwrap();

                let transaction = self.context.solana_rpc.get_transaction(&sig).await?;
                if let Some(meta) = &transaction.transaction.meta {
                    if let Some(err) = &meta.err {
                        tracing::error!("Transaction error: {:?}", err);
                        return proof;
                    }
                }

                let transaction: TransactionInfo =
                    FetchedEncodedTransactionWithStatusMeta(transaction).try_into()?;

                program_transformer.handle_transaction(&transaction).await?;

                // Re-check proof after handling transaction
                get_proof_for_asset(&db, asset.to_bytes().to_vec())
                    .await
                    .map_or_else(|_| Ok(ProofResult::NotFound), leaf_proof_result)
            }
            _ => proof,
        }
    }
}
