use std::io::Cursor;

use crate::{error::IngesterError, tasks::TaskData};
use base64::engine::general_purpose;
use borsh::BorshDeserialize;
use plerkle_serialization::{CompiledInstruction, Pubkey};

use blockbuster::instruction::InstructionBundle;
use hpl_toolkit::SchemaValue;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ActiveValue::Set, ConnectionTrait,
    DatabaseConnection, DbBackend, EntityTrait,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    message::Message,
    pubkey,
    signature::Keypair,
    transaction::Transaction,
};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

async fn extract_account_schema_values<'a>(
    program: &'a Pubkey,
    ix: &'a CompiledInstruction<'a>,
    keys: &'a [Pubkey],
    rpc_client: &'a RpcClient,
    directory: &'a mut HashMap<pubkey::Pubkey, SchemaValue>,
) {
    let program_id = pubkey::Pubkey::from(program.0);
    if let Some(accounts) = ix.accounts() {
        let metas = accounts
            .iter()
            .filter_map(|account| {
                let pubkey = pubkey::Pubkey::from(keys[account as usize].0);
                if directory.contains_key(&pubkey) {
                    return None;
                }

                Some(AccountMeta {
                    pubkey,
                    is_signer: false,
                    is_writable: false,
                })
            })
            .collect::<Vec<AccountMeta>>();

        if metas.len() == 0 {
            return;
        }

        let simulation_ix = Instruction {
            program_id,
            accounts: metas.to_owned(),
            data: vec![215, 120, 181, 56, 249, 195, 139, 167], // discriminator for __account_schemas ix
        };
        let message = Message::new(&[simulation_ix], None);
        let tx: Transaction = Transaction::new(&Vec::<&Keypair>::new(), message, Hash::default());
        if let Ok(res) = rpc_client.simulate_transaction(&tx).await {
            if let Some(return_data) = res.value.return_data {
                let mut wrapped_reader = Cursor::new(return_data.data.0.as_bytes());
                let mut decoder = base64::read::DecoderReader::new(
                    &mut wrapped_reader,
                    &general_purpose::STANDARD,
                );

                let schema_values = Vec::<Option<SchemaValue>>::deserialize_reader(&mut decoder)
                    .unwrap_or_default();
                let mut i = 0;
                schema_values.into_iter().for_each(|schema_value| {
                    if let Some(schema_value) = schema_value {
                        let k = metas[i].pubkey;
                        directory.insert(k, schema_value);
                    }
                    i += 1;
                })
            }
        }
    }
}

pub async fn etl_account_schema_values<'a, 'c>(
    ix_bundle: &'a InstructionBundle<'a>,
    db: &'c DatabaseConnection,
    rpc_client: &'a RpcClient,
    _task_manager: &UnboundedSender<TaskData>,
) -> Result<(), IngesterError> {
    let mut directory = HashMap::<pubkey::Pubkey, SchemaValue>::new();

    let keys = ix_bundle.keys;

    if let Some(ix) = ix_bundle.instruction {
        extract_account_schema_values(&ix_bundle.program, &ix, keys, rpc_client, &mut directory)
            .await;
    }

    if let Some(inner_ixs) = &ix_bundle.inner_ix {
        for (program_id, ix) in inner_ixs {
            extract_account_schema_values(
                &program_id,
                &ix,
                keys,
                rpc_client,
                &mut directory,
            )
            .await;
        }
    }

    // let program_id = Pubkey::try_from(account_update.owner().unwrap().0.as_slice())
    //     .map_err(|_| IngesterError::PubkeyReadError)?;

    // let address = Pubkey::try_from(account_update.pubkey().unwrap().0.as_slice())
    //     .map_err(|_| IngesterError::PubkeyReadError)?;

    // Instruction {
    //     program_id,
    //     accounts: vec![AccountMeta {
    //         pubkey: address,
    //         is_signer: false,
    //         is_writable: false,
    //     }],
    //     data: vec![170, 16, 100, 40, 210, 26, 34, 65], // discriminator for __account_schemas ix
    // };

    // let spl_token_program = account_update.owner().unwrap().0.to_vec();
    // match &parsing_result {
    //     TokenProgramAccount::TokenAccount(ta) => {
    //         let mint = ta.mint.to_bytes().to_vec();
    //         let delegate: Option<Vec<u8>> = match ta.delegate {
    //             COption::Some(d) => Some(d.to_bytes().to_vec()),
    //             COption::None => None,
    //         };
    //         let frozen = matches!(ta.state, AccountState::Frozen);
    //         let owner = ta.owner.to_bytes().to_vec();
    //         let model = token_accounts::ActiveModel {
    //             pubkey: Set(key_bytes),
    //             mint: Set(mint.clone()),
    //             delegate: Set(delegate.clone()),
    //             owner: Set(owner.clone()),
    //             frozen: Set(frozen),
    //             delegated_amount: Set(ta.delegated_amount as i64),
    //             token_program: Set(spl_token_program),
    //             slot_updated: Set(account_update.slot() as i64),
    //             amount: Set(ta.amount as i64),
    //             close_authority: Set(None),
    //         };

    //         let mut query = token_accounts::Entity::insert(model)
    //             .on_conflict(
    //                 OnConflict::columns([token_accounts::Column::Pubkey])
    //                     .update_columns([
    //                         token_accounts::Column::Mint,
    //                         token_accounts::Column::DelegatedAmount,
    //                         token_accounts::Column::Delegate,
    //                         token_accounts::Column::Amount,
    //                         token_accounts::Column::Frozen,
    //                         token_accounts::Column::TokenProgram,
    //                         token_accounts::Column::Owner,
    //                         token_accounts::Column::CloseAuthority,
    //                         token_accounts::Column::SlotUpdated,
    //                     ])
    //                     .to_owned(),
    //             )
    //             .build(DbBackend::Postgres);
    //         query.sql = format!(
    //             "{} WHERE excluded.slot_updated > token_accounts.slot_updated",
    //             query.sql
    //         );
    //         db.execute(query).await?;
    //         let txn = db.begin().await?;
    //         let asset_update: Option<asset::Model> = asset::Entity::find_by_id(mint)
    //             .filter(asset::Column::OwnerType.eq("single"))
    //             .one(&txn)
    //             .await?;
    //         if let Some(asset) = asset_update {
    //             // will only update owner if token account balance is non-zero
    //             // since the asset is marked as single then the token account balance can only be 1. Greater implies a fungible token in which case no si
    //             // TODO: this does not guarantee in case when wallet receives an amount of 1 for a token but its supply is more. is unlikely since mints often have a decimal
    //             if ta.amount == 1 {
    //                 let mut active: asset::ActiveModel = asset.into();
    //                 active.owner = Set(Some(owner));
    //                 active.delegate = Set(delegate);
    //                 active.frozen = Set(frozen);
    //                 active.save(&txn).await?;
    //             }
    //         }
    //         txn.commit().await?;
    //         Ok(())
    //     }
    //     TokenProgramAccount::Mint(m) => {
    //         let freeze_auth: Option<Vec<u8>> = match m.freeze_authority {
    //             COption::Some(d) => Some(d.to_bytes().to_vec()),
    //             COption::None => None,
    //         };
    //         let mint_auth: Option<Vec<u8>> = match m.mint_authority {
    //             COption::Some(d) => Some(d.to_bytes().to_vec()),
    //             COption::None => None,
    //         };
    //         let model = tokens::ActiveModel {
    //             mint: Set(key_bytes.clone()),
    //             token_program: Set(spl_token_program),
    //             slot_updated: Set(account_update.slot() as i64),
    //             supply: Set(m.supply as i64),
    //             decimals: Set(m.decimals as i32),
    //             close_authority: Set(None),
    //             extension_data: Set(None),
    //             mint_authority: Set(mint_auth),
    //             freeze_authority: Set(freeze_auth),
    //         };

    //         let mut query = tokens::Entity::insert(model)
    //             .on_conflict(
    //                 OnConflict::columns([tokens::Column::Mint])
    //                     .update_columns([
    //                         tokens::Column::Supply,
    //                         tokens::Column::TokenProgram,
    //                         tokens::Column::MintAuthority,
    //                         tokens::Column::CloseAuthority,
    //                         tokens::Column::ExtensionData,
    //                         tokens::Column::SlotUpdated,
    //                         tokens::Column::Decimals,
    //                         tokens::Column::FreezeAuthority,
    //                     ])
    //                     .to_owned(),
    //             )
    //             .build(DbBackend::Postgres);
    //         query.sql = format!(
    //             "{} WHERE excluded.slot_updated > tokens.slot_updated",
    //             query.sql
    //         );
    //         db.execute(query).await?;
    //         let asset_update: Option<asset::Model> = asset::Entity::find_by_id(key_bytes.clone())
    //             .filter(asset::Column::OwnerType.eq("single"))
    //             .one(db)
    //             .await?;
    //         if let Some(asset) = asset_update {
    //             let mut active: asset::ActiveModel = asset.into();
    //             active.supply = Set(m.supply as i64);
    //             active.supply_mint = Set(Some(key_bytes));
    //             active.save(db).await?;
    //         }
    //         Ok(())
    //     }
    //     _ => Err(IngesterError::NotImplemented),
    // }?;
    Ok(())
}
