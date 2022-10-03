use blockbuster::programs::token_account::TokenProgramAccount;
use crate::{BgTask, IngesterError};
use blockbuster::programs::token_metadata::{TokenMetadataAccountData, TokenMetadataAccountState};
use plerkle_serialization::AccountInfo;
use tokio::sync::mpsc::UnboundedSender;
use digital_asset_types::dao::{asset, token_accounts, tokens};
use sea_orm::{entity::*, query::*, sea_query::OnConflict, ActiveValue::{Set}, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, EntityTrait, JsonValue, DatabaseConnection};
use solana_sdk::program_option::COption;
use spl_token::state::AccountState;
use crate::program_transformers::token;

pub async fn handle_token_program_account<'a, 'b, 'c>(
    account_update: &'a AccountInfo<'a>,
    parsing_result: &'b TokenProgramAccount,
    db: &'c DatabaseConnection,
    task_manager: &UnboundedSender<Box<dyn BgTask>>,
) -> Result<(), IngesterError> {
    let txn = db.begin().await?;
    let key = account_update.pubkey().unwrap().clone();
    let key_bytes = key.0.to_vec();
    let spl_token_program = account_update.owner().unwrap().0.to_vec();
    match &parsing_result {
        TokenProgramAccount::TokenAccount(ta) => {
            let mint = ta.mint.to_bytes().to_vec();
            let delegate: Option<Vec<u8>> = match ta.delegate {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let frozen = match ta.state {
                AccountState::Frozen => true,
                _ => false,
            };
            let owner = ta.owner.to_bytes().to_vec();
            let model = token_accounts::ActiveModel {
                pubkey: Set(key_bytes),
                mint: Set(Some(mint.clone())),
                delegate: Set(delegate),
                owner: Set(owner.clone()),
                frozen: Set(frozen),
                delegated_amount: Set(ta.delegated_amount as i64),
                token_program: Set(spl_token_program),
                slot_updated: Set(account_update.slot() as i64),
                amount: Set(ta.amount as i64),
                close_authority: Set(None),
            };

            let query = token_accounts::Entity::insert(model).on_conflict(OnConflict::columns(
                [token_accounts::Column::Pubkey],
            ).update_columns([
                token_accounts::Column::DelegatedAmount,
                token_accounts::Column::Delegate,
                token_accounts::Column::Amount,
                token_accounts::Column::Frozen,
                token_accounts::Column::Owner,
                token_accounts::Column::CloseAuthority,
                token_accounts::Column::SlotUpdated,
            ]).to_owned())
                .build(DbBackend::Postgres);
            txn.execute(query).await?;
            let asset_update: Option<asset::Model> = asset::Entity::find_by_id(mint)
                .filter(asset::Column::OwnerType.eq("single"))
                .one(&txn).await?;
            if let Some(asset) = asset_update {
                let mut active: asset::ActiveModel = asset.into();
                active.owner = Set(Some(owner));
                active.save(&txn).await?;
            }
            Ok(())
        }
        TokenProgramAccount::Mint(m) => {
            let freeze_auth: Option<Vec<u8>> = match m.freeze_authority {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let mint_auth: Option<Vec<u8>> = match m.mint_authority {
                COption::Some(d) => Some(d.to_bytes().to_vec()),
                COption::None => None,
            };
            let model = tokens::ActiveModel {
                mint: Set(key_bytes),
                token_program: Set(spl_token_program),
                slot_updated: Set(account_update.slot() as i64),
                supply: Set(m.supply as i64),
                decimals: Set(m.decimals as i32),
                close_authority: Set(None),
                extension_data: Set(None),
                mint_authority: Set(mint_auth),
                freeze_authority: Set(freeze_auth),
            };

            let query = tokens::Entity::insert(model).on_conflict(OnConflict::columns(
                [tokens::Column::Mint],
            ).update_columns([
                tokens::Column::Supply,
                tokens::Column::MintAuthority,
                tokens::Column::CloseAuthority,
                tokens::Column::ExtensionData,
                tokens::Column::SlotUpdated,
                tokens::Column::Decimals
            ]).to_owned())
                .build(DbBackend::Postgres);
            txn.execute(query).await?;
            Ok(())
        }
        _ => Err(IngesterError::NotImplemented),
    }?;
    txn.commit().await?;
    Ok(())
}
