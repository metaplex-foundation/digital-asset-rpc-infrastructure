use crate::error::ErrorKind;
use anyhow::Result;
use clap::Parser;
use das_core::Rpc;
use futures::{stream::FuturesUnordered, StreamExt};
use log::error;
use program_transformers::TransactionInfo;
use solana_program::pubkey::Pubkey;
use solana_sdk::instruction::CompiledInstruction;
use solana_sdk::signature::Signature;
use solana_sdk::transaction::VersionedTransaction;
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    InnerInstruction, InnerInstructions, UiInstruction,
};
use tokio::{
    sync::mpsc::{channel, Sender},
    task::JoinHandle,
};

pub struct PubkeyString(pub String);

impl TryFrom<PubkeyString> for Pubkey {
    type Error = ErrorKind;

    fn try_from(value: PubkeyString) -> Result<Self, Self::Error> {
        let decoded_bytes = bs58::decode(value.0)
            .into_vec()
            .map_err(|e| ErrorKind::Generic(e.to_string()))?;

        Pubkey::try_from(decoded_bytes)
            .map_err(|_| ErrorKind::Generic("unable to convert pubkey".to_string()))
    }
}

pub struct FetchedEncodedTransactionWithStatusMeta(pub EncodedConfirmedTransactionWithStatusMeta);

impl TryFrom<FetchedEncodedTransactionWithStatusMeta> for TransactionInfo {
    type Error = ErrorKind;

    fn try_from(
        fetched_transaction: FetchedEncodedTransactionWithStatusMeta,
    ) -> Result<Self, Self::Error> {
        let mut account_keys = Vec::new();
        let encoded_transaction_with_status_meta = fetched_transaction.0;

        let ui_transaction: VersionedTransaction = encoded_transaction_with_status_meta
            .transaction
            .transaction
            .decode()
            .ok_or(ErrorKind::Generic(
                "unable to decode transaction".to_string(),
            ))?;

        let signature = ui_transaction.signatures[0];

        let msg = ui_transaction.message;

        let meta = encoded_transaction_with_status_meta
            .transaction
            .meta
            .ok_or(ErrorKind::Generic(
                "unable to get meta from transaction".to_string(),
            ))?;

        for address in msg.static_account_keys().iter().copied() {
            account_keys.push(address);
        }
        let ui_loaded_addresses = meta.loaded_addresses;

        let message_address_table_lookup = msg.address_table_lookups();

        if message_address_table_lookup.is_some() {
            if let OptionSerializer::Some(ui_lookup_table) = ui_loaded_addresses {
                for address in ui_lookup_table.writable {
                    account_keys.push(PubkeyString(address).try_into()?);
                }

                for address in ui_lookup_table.readonly {
                    account_keys.push(PubkeyString(address).try_into()?);
                }
            }
        }

        let mut meta_inner_instructions = Vec::new();

        let compiled_instruction = msg.instructions().to_vec();

        let mut instructions = Vec::new();

        for inner in compiled_instruction {
            instructions.push(InnerInstruction {
                stack_height: Some(0),
                instruction: CompiledInstruction {
                    program_id_index: inner.program_id_index,
                    accounts: inner.accounts,
                    data: inner.data,
                },
            });
        }

        meta_inner_instructions.push(InnerInstructions {
            index: 0,
            instructions,
        });

        if let OptionSerializer::Some(inner_instructions) = meta.inner_instructions {
            for ix in inner_instructions {
                let mut instructions = Vec::new();

                for inner in ix.instructions {
                    if let UiInstruction::Compiled(compiled) = inner {
                        instructions.push(InnerInstruction {
                            stack_height: compiled.stack_height,
                            instruction: CompiledInstruction {
                                program_id_index: compiled.program_id_index,
                                accounts: compiled.accounts,
                                data: bs58::decode(compiled.data)
                                    .into_vec()
                                    .map_err(|e| ErrorKind::Generic(e.to_string()))?,
                            },
                        });
                    }
                }

                meta_inner_instructions.push(InnerInstructions {
                    index: ix.index,
                    instructions,
                });
            }
        }

        Ok(Self {
            slot: encoded_transaction_with_status_meta.slot,
            account_keys,
            signature,
            message_instructions: msg.instructions().to_vec(),
            meta_inner_instructions,
        })
    }
}

#[derive(Parser, Clone, Debug)]
pub struct SignatureWorkerArgs {
    /// The size of the signature channel.
    #[arg(long, env, default_value = "100000")]
    pub signature_channel_size: usize,
    /// The number of transaction workers.
    #[arg(long, env, default_value = "50")]
    pub signature_worker_count: usize,
}

impl SignatureWorkerArgs {
    pub fn start(
        &self,
        context: crate::BubblegumBackfillContext,
        forwarder: Sender<TransactionInfo>,
    ) -> Result<(JoinHandle<()>, Sender<Signature>)> {
        let (sig_sender, mut sig_receiver) = channel::<Signature>(self.signature_channel_size);
        let worker_count = self.signature_worker_count;

        let handle = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();

            while let Some(signature) = sig_receiver.recv().await {
                if handlers.len() >= worker_count {
                    handlers.next().await;
                }

                let solana_rpc = context.solana_rpc.clone();
                let transaction_sender = forwarder.clone();

                let handle = spawn_transaction_worker(solana_rpc, transaction_sender, signature);

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;
        });

        Ok((handle, sig_sender))
    }
}

async fn queue_transaction<'a>(
    client: Rpc,
    sender: Sender<TransactionInfo>,
    signature: Signature,
) -> Result<(), ErrorKind> {
    let transaction = client.get_transaction(&signature).await?;

    sender
        .send(FetchedEncodedTransactionWithStatusMeta(transaction).try_into()?)
        .await
        .map_err(|e| ErrorKind::Generic(e.to_string()))?;

    Ok(())
}

fn spawn_transaction_worker(
    client: Rpc,
    sender: Sender<TransactionInfo>,
    signature: Signature,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = queue_transaction(client, sender, signature).await {
            error!("queue transaction: {:?}", e);
        }
    })
}
