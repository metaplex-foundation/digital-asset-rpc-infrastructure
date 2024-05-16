use std::collections::HashSet;

use crate::error::IngesterError;
use base64::Engine;
use borsh::BorshDeserialize;
use plerkle_serialization::TransactionInfo;

use anchor_lang::Discriminator;
use async_trait::async_trait;
use digital_asset_types::dao::accounts;
use hpl_toolkit::schema::{AccountSchemaValue, ToSchema};
use log::{debug, error, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ConnectionTrait, DatabaseConnection,
    DbBackend, EntityTrait,
};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcSimulateTransactionConfig},
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey as pubkey_macro,
    pubkey::Pubkey,
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use std::collections::HashMap;

mod hpl_character_manager;
mod hpl_nectar_missions;

#[async_trait]
pub trait IndexablePrograms {
    fn keys(&self) -> &Vec<Pubkey>;

    fn rpc_client(&self) -> &RpcClient;

    async fn populate_programs(&mut self);

    async fn select_and_group_accounts<'a>(
        &self,
        tx: &'a TransactionInfo<'a>,
    ) -> HashMap<Pubkey, HashSet<Pubkey>> {
        info!("Fetching account infos to check for updates");
        let pubkeys = tx
            .account_keys()
            .unwrap()
            .iter()
            .map(|p| Pubkey::from(p.0))
            .collect::<Vec<_>>();
        let accounts_response = self
            .rpc_client()
            .get_multiple_accounts_with_config(
                pubkeys.as_slice(),
                RpcAccountInfoConfig {
                    data_slice: Some(solana_account_decoder::UiDataSliceConfig {
                        offset: 0,
                        length: 0,
                    }),
                    ..RpcAccountInfoConfig::default()
                },
            )
            .await;

        let mut programs = HashMap::<Pubkey, HashSet<Pubkey>>::new();
        self.keys().iter().for_each(|program| {
            programs.insert(program.to_owned(), HashSet::<Pubkey>::new());
        });

        if let Ok(accounts_response) = accounts_response {
            info!("Compiling accounts into program directory");
            accounts_response
                .value
                .iter()
                .enumerate()
                .for_each(|(i, account)| {
                    if let Some(account) = account {
                        let set = programs.get_mut(&account.owner);
                        if let Some(set) = set {
                            let account_key = pubkeys[i];
                            set.insert(account_key);
                        }
                    }
                })
        } else if let Err(e) = accounts_response {
            error!("Couldn't find accounts {:?}", e);
        }

        programs
    }

    async fn index_tx_accounts<'a>(
        &self,
        tx: &'a TransactionInfo<'a>,
        db: &'a DatabaseConnection,
    ) -> Result<(), IngesterError> {
        let sig = tx.signature();
        let rpc_client = self.rpc_client();
        let mut remaining_tries = 200u64;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            let current_slot = rpc_client
                .get_slot_with_commitment(rpc_client.commitment())
                .await;
            let current_slot = current_slot.unwrap_or(0);

            info!(
                "Checking confirmation for tx: {:?}, current_slot: {}, tx_slot: {}",
                sig,
                current_slot,
                tx.slot()
            );

            if current_slot >= tx.slot() {
                info!(
                    "Fetching account values for tx: {:?}, remaining tries: {}",
                    sig, remaining_tries
                );
                etl_account_schema_values(
                    self.select_and_group_accounts(tx).await,
                    Some(Pubkey::from(tx.account_keys().unwrap().get(0).0)),
                    tx.slot(),
                    db,
                    rpc_client,
                )
                .await
                .map_err(|err| {
                    error!(
                        "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                        sig, err
                    );
                    err
                })?;
                break;
            }

            if remaining_tries == 0 {
                info!(
                    "Coudn't confirm, tx: {:?}, current_slot: {}, tx_slot: {}",
                    sig,
                    current_slot,
                    tx.slot()
                );
                break;
            }
            remaining_tries -= 1;
        }

        Ok(())
    }
}

async fn extract_account_schema_values<'a>(
    program_id: Pubkey,
    pubkey: Pubkey,
    payer: &'a Option<Pubkey>,
    rpc_client: &'a RpcClient,
    directory: &'a mut HashMap<Pubkey, AccountSchemaValue>,
) {
    // FOR CHARACTER MANAGER
    if program_id == pubkey_macro!("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg") {
        debug!("Found character manager account");
        let data = rpc_client.get_account_data(&pubkey).await.unwrap();
        let (disc_bytes, data) = data.split_at(8);
        let mut disc = [0u8; 8];
        disc.copy_from_slice(disc_bytes);
        let mut matched = true;
        match disc {
            hpl_character_manager::AssemblerConfig::DISCRIMINATOR => {
                let schema = hpl_character_manager::AssemblerConfig::deserialize(&mut &data[..])
                    .unwrap()
                    .schema_value();
                directory.insert(
                    pubkey,
                    AccountSchemaValue {
                        address: pubkey,
                        program_id,
                        discriminator: disc,
                        value: schema,
                    },
                );
            }
            hpl_character_manager::CharacterModel::DISCRIMINATOR => {
                let schema = hpl_character_manager::CharacterModel::deserialize(&mut &data[..])
                    .unwrap()
                    .schema_value();
                directory.insert(
                    pubkey,
                    AccountSchemaValue {
                        address: pubkey,
                        program_id,
                        discriminator: disc,
                        value: schema,
                    },
                );
            }
            _ => matched = false,
        }

        if matched {
            return;
        } else {
            debug!("No discriminators matched moving forward to simulation");
        }
    } else if program_id == pubkey_macro!("HuntaX1CmUt5EByyFPE8pMf13SpvezybmMTtjmpmGmfj") {
        debug!("Found Mission Account");

        let data = rpc_client.get_account_data(&pubkey).await.unwrap();
        let (disc_bytes, data) = data.split_at(8);
        let mut disc = [0u8; 8];
        disc.copy_from_slice(disc_bytes);

        let mut matched = true;
        match disc {
            hpl_nectar_missions::MissionPool::DISCRIMINATOR => {
                let schema = hpl_nectar_missions::MissionPool::deserialize(&mut &data[..])
                    .unwrap()
                    .schema_value();
                directory.insert(
                    pubkey,
                    AccountSchemaValue {
                        address: pubkey,
                        program_id,
                        discriminator: disc,
                        value: schema,
                    },
                );
            }
            hpl_nectar_missions::Mission::DISCRIMINATOR => {
                let schema = hpl_nectar_missions::Mission::deserialize(&mut &data[..])
                    .unwrap()
                    .schema_value();
                directory.insert(
                    pubkey,
                    AccountSchemaValue {
                        address: pubkey,
                        program_id,
                        discriminator: disc,
                        value: schema,
                    },
                );
            }
            _ => matched = false,
        }

        if matched {
            return;
        } else {
            debug!("No discriminators matched moving forward to simulation");
        }
    }

    if payer.is_none() {
        debug!("Payer is none, aborting");
        return;
    }

    let metas = vec![AccountMeta {
        pubkey: pubkey,
        is_signer: false,
        is_writable: false,
    }];

    if metas.len() == 0 {
        return;
    }

    let compute_unit_ix =
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(1_000_000);
    let simulation_ix = Instruction {
        program_id,
        accounts: metas.to_owned(),
        data: vec![215, 120, 181, 56, 249, 195, 139, 167], // discriminator for __account_schemas ix
    };
    // let message = Message::new(&[simulation_ix], Some(&metas[0].pubkey));
    // let mut tx: Transaction =
    //     Transaction::new_with_payer(&[simulation_ix], Some(&metas[0].pubkey));
    let tx: Transaction =
        Transaction::new_with_payer(&[compute_unit_ix, simulation_ix], payer.as_ref());
    info!("Payer: {:?}", payer.as_ref());

    // tx.message.recent_blockhash = hash;
    info!("Simulating Tx {:?}", program_id,);

    match rpc_client
        .simulate_transaction_with_config(
            &tx,
            RpcSimulateTransactionConfig {
                sig_verify: false,
                commitment: Some(rpc_client.commitment()),
                replace_recent_blockhash: true,
                encoding: Some(UiTransactionEncoding::Base58),
                ..RpcSimulateTransactionConfig::default()
            },
        )
        .await
    {
        Ok(res) => {
            info!("Tx Simualted success {:?}", res.value);
            if let Some(return_data) = res.value.return_data {
                info!(
                    "Simulate Response {:?} {}",
                    return_data.data.1, return_data.data.0
                );
                let all_bytes = base64::engine::general_purpose::STANDARD
                    .decode(return_data.data.0)
                    .unwrap();

                let (_, bytes) = all_bytes.split_at(4);

                // let mut len_bytes = [0u8; 4];
                // len_bytes.copy_from_slice(&bytes[0..4]);
                // let len: usize = u32::from_le_bytes(len_bytes) as usize;
                // bytes = [bytes, vec![0; len].as_slice()].concat();
                info!("Bytes {:?}", bytes);

                match Vec::<Option<AccountSchemaValue>>::deserialize_reader(&mut &bytes[..]) {
                    Ok(schema_values) => {
                        let mut i = 0;
                        schema_values.into_iter().for_each(|schema_value| {
                            if let Some(schema_value) = schema_value {
                                let k = metas[i].pubkey;
                                directory.insert(k, schema_value);
                            }
                            i += 1;
                        })
                    }
                    Err(error) => error!("Error deserialize_reader {:?}", error),
                }
            } else {
                error!("Tx Simualted no response {:?}", res.value);
            }
        }
        Err(err) => error!("Tx simulation failed {}", err),
    }
}

pub async fn etl_account_schema_values<'a>(
    program_accounts: HashMap<Pubkey, HashSet<Pubkey>>,
    payer: Option<Pubkey>,
    slot: u64,
    db: &'a DatabaseConnection,
    rpc_client: &'a RpcClient,
) -> Result<(), IngesterError> {
    info!("Fetching accounts latest schema");
    let mut directory = HashMap::<Pubkey, AccountSchemaValue>::new();

    for (program, pubkeys) in program_accounts {
        for pubkey in pubkeys {
            extract_account_schema_values(program, pubkey, &payer, rpc_client, &mut directory)
                .await;
        }
    }

    let accounts_schemas = directory.values();

    info!("Found {} account updates", accounts_schemas.len());

    if accounts_schemas.len() > 0 {
        info!("Updated accounts found building query");
        let models = accounts_schemas
            .into_iter()
            .map(|account| accounts::ActiveModel {
                id: Set(account.address.to_bytes().to_vec()),
                program_id: Set(account.program_id.to_bytes().to_vec()),
                discriminator: Set(account.discriminator.to_vec()),
                parsed_data: Set(account.value.to_owned().into()),
                slot_updated: Set(slot as i64),
                ..Default::default()
            })
            .collect::<Vec<accounts::ActiveModel>>();

        let query = accounts::Entity::insert_many(models)
            .on_conflict(
                OnConflict::columns([accounts::Column::Id])
                    .update_columns([
                        accounts::Column::ProgramId,
                        accounts::Column::Discriminator,
                        accounts::Column::ParsedData,
                        accounts::Column::SlotUpdated,
                    ])
                    .to_owned(),
            )
            .build(DbBackend::Postgres);

        info!("Query builed successfully");
        db.execute(query)
            .await
            .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
        info!("Query executed successfully");
    }

    Ok(())
}
