use anyhow::Result;
use blockbuster::{
    instruction::InstructionBundle, program_handler::ProgramParser,
    programs::bubblegum::BubblegumParser, programs::ProgramParseResult,
};
use clap::Parser;
use das_core::{create_download_metadata_notifier, DownloadMetadataInfo};
use log::error;
use program_transformers::{
    bubblegum::handle_bubblegum_instruction, ProgramTransformer, TransactionInfo,
};
use sea_orm::SqlxPostgresConnector;
use tokio::sync::mpsc::{channel, Sender, UnboundedSender};
use tokio::task::JoinHandle;

use crate::BubblegumBackfillContext;

#[derive(Parser, Debug, Clone)]
pub struct ProgramTransformerWorkerArgs {
    #[arg(long, env, default_value = "100000")]
    pub program_transformer_channel_size: usize,
}

impl ProgramTransformerWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumBackfillContext,
        forwarder: UnboundedSender<DownloadMetadataInfo>,
    ) -> Result<(JoinHandle<()>, Sender<TransactionInfo>)> {
        let (sender, mut receiver) =
            channel::<TransactionInfo>(self.program_transformer_channel_size);

        let worker_forwarder = forwarder.clone();
        let worker_pool = context.database_pool.clone();
        let handle = tokio::spawn(async move {
            let mut transactions = Vec::new();

            let download_metadata_notifier =
                create_download_metadata_notifier(worker_forwarder.clone()).await;
            let program_transformer =
                ProgramTransformer::new(worker_pool.clone(), download_metadata_notifier);

            while let Some(transaction) = receiver.recv().await {
                transactions.push(transaction);
            }

            let mut instructions = transactions
                .iter()
                .flat_map(|tx_info| {
                    let ordered_instructions = program_transformer.break_transaction(tx_info);
                    ordered_instructions.into_iter().map(|(ix_pair, inner_ix)| {
                        (
                            tx_info.signature.to_string(),
                            ix_pair.0,
                            ix_pair.1,
                            inner_ix,
                            ix_pair
                                .1
                                .accounts
                                .iter()
                                .map(|&i| tx_info.account_keys[i as usize])
                                .collect::<Vec<_>>(),
                            tx_info.slot,
                        )
                    })
                })
                .collect::<Vec<_>>();
            instructions.sort_by(|a, b| {
                let a_tree_update_seq = if let Some(program_parser) =
                    program_transformer.match_program(&a.1)
                {
                    if let Ok(result) = program_parser.handle_instruction(&InstructionBundle {
                        txn_id: &a.0,
                        program: a.1,
                        instruction: Some(a.2),
                        inner_ix: a.3.as_deref(),
                        keys: a.4.as_slice(),
                        slot: a.5,
                    }) {
                        if let ProgramParseResult::Bubblegum(parsing_result) = result.result_type()
                        {
                            parsing_result
                                .tree_update
                                .as_ref()
                                .map_or(u64::MAX, |event| event.seq)
                        } else {
                            u64::MAX
                        }
                    } else {
                        u64::MAX
                    }
                } else {
                    u64::MAX
                };

                let b_tree_update_seq = if let Some(program_parser) =
                    program_transformer.match_program(&b.1)
                {
                    if let Ok(result) = program_parser.handle_instruction(&InstructionBundle {
                        txn_id: &b.0,
                        program: b.1,
                        instruction: Some(b.2),
                        inner_ix: b.3.as_deref(),
                        keys: b.4.as_slice(),
                        slot: b.5,
                    }) {
                        if let ProgramParseResult::Bubblegum(parsing_result) = result.result_type()
                        {
                            parsing_result
                                .tree_update
                                .as_ref()
                                .map_or(u64::MAX, |event| event.seq)
                        } else {
                            u64::MAX
                        }
                    } else {
                        u64::MAX
                    }
                } else {
                    u64::MAX
                };

                a_tree_update_seq.cmp(&b_tree_update_seq)
            });

            let parser = BubblegumParser {};

            let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(worker_pool);

            for i in instructions {
                let bundle = &InstructionBundle {
                    txn_id: &i.0,
                    program: i.1,
                    instruction: Some(i.2),
                    inner_ix: i.3.as_deref(),
                    keys: i.4.as_slice(),
                    slot: i.5,
                };
                if let Ok(result) = parser.handle_instruction(bundle) {
                    if let ProgramParseResult::Bubblegum(parsing_result) = result.result_type() {
                        let download_metadata_notifier =
                            create_download_metadata_notifier(worker_forwarder.clone()).await;

                        if let Err(err) = handle_bubblegum_instruction(
                            parsing_result,
                            bundle,
                            &conn,
                            &download_metadata_notifier,
                        )
                        .await
                        {
                            error!(
                                "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                                bundle.txn_id, err
                            );
                            break;
                        }
                    }
                }
            }
        });

        Ok((handle, sender))
    }
}
