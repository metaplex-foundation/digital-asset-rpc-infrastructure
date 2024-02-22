use std::{collections::HashSet, str::FromStr};

use crate::{error::IngesterError, tasks::TaskData};
use base64::Engine;
use borsh::BorshDeserialize;
use plerkle_serialization::Pubkey;

use digital_asset_types::dao::accounts;
use hpl_toolkit::AccountSchemaValue;
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
    pubkey,
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

async fn extract_account_schema_values<'a>(
    program_id: pubkey::Pubkey,
    pubkeys: HashSet<pubkey::Pubkey>,
    payer: &'a Option<pubkey::Pubkey>,
    rpc_client: &'a RpcClient,
    directory: &'a mut HashMap<pubkey::Pubkey, AccountSchemaValue>,
) {
    if payer.is_none() {
        debug!("Payer is none, aborting");
        return;
    }

    let metas = pubkeys
        .into_iter()
        .map(|pubkey| AccountMeta {
            pubkey: pubkey,
            is_signer: false,
            is_writable: false,
        })
        .collect::<Vec<AccountMeta>>();

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
                info!("Tx Simualted no response");
            }
        }
        Err(err) => error!("Tx simulation failed {}", err),
    }
}

pub async fn etl_account_schema_values<'a, 'c>(
    allowed_programs: &Vec<pubkey::Pubkey>,
    accounts: &'a [Pubkey],
    slot: u64,
    payer: &'a Option<pubkey::Pubkey>,
    db: &'c DatabaseConnection,
    rpc_client: &'a RpcClient,
    _task_manager: &UnboundedSender<TaskData>,
) -> Result<(), IngesterError> {
    info!("Fetching account infos to check for updates");
    let pubkeys = accounts
        .iter()
        .map(|p| pubkey::Pubkey::from(p.0))
        .collect::<Vec<_>>();
    let accounts_response = rpc_client
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

    let mut programs = HashMap::<String, HashSet<pubkey::Pubkey>>::new();
    allowed_programs.iter().for_each(|program| {
        programs.insert(program.to_string(), HashSet::<pubkey::Pubkey>::new());
    });

    if let Ok(accounts_response) = accounts_response {
        info!("Compiling accounts into program directory");
        accounts_response
            .value
            .iter()
            .enumerate()
            .for_each(|(i, account)| {
                if let Some(account) = account {
                    let set = programs.get_mut(&account.owner.to_string());
                    if let Some(set) = set {
                        let account_key = pubkeys[i];
                        set.insert(account_key);
                    }
                }
            })
    } else if let Err(e) = accounts_response {
        error!("Couldn't find accounts {:?}", e);
    }

    info!("Checking instructions for account update");
    let mut directory = HashMap::<pubkey::Pubkey, AccountSchemaValue>::new();

    for (program, pubkeys) in programs {
        extract_account_schema_values(
            pubkey::Pubkey::from_str(program.as_str()).unwrap(),
            pubkeys,
            payer,
            rpc_client,
            &mut directory,
        )
        .await;
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
