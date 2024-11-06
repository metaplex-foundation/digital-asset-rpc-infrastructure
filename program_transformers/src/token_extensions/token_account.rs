use crate::{
    error::IngesterError,
    metric,
    program_transformers::{
        asset_upserts::{upsert_assets_token_account_columns, AssetTokenAccountColumns},
        token::upsert_owner_for_token_account,
    },
};
use blockbuster::programs::token_extensions::TokenAccount;
use cadence_macros::{is_global_default_set, statsd_count};
use digital_asset_types::dao::asset;
use plerkle_serialization::AccountInfo;
use sea_orm::{entity::*, query::*, ActiveValue::Set, DatabaseConnection, EntityTrait};
use solana_sdk::program_option::COption;
use spl_token_2022::state::AccountState;

pub async fn handle_token2022_token_account<'a, 'b, 'c>(
    ta: &TokenAccount,
    account_update: &'a AccountInfo<'a>,
    db: &'c DatabaseConnection,
) -> Result<(), IngesterError> {
    let key = *account_update.pubkey().unwrap();
    let key_bytes = key.0.to_vec();
    let spl_token_program = account_update.owner().unwrap().0.to_vec();

    let mint = ta.account.mint.to_bytes().to_vec();
    let delegate: Option<Vec<u8>> = match ta.account.delegate {
        COption::Some(d) => Some(d.to_bytes().to_vec()),
        COption::None => None,
    };
    let frozen = match ta.account.state {
        AccountState::Frozen => true,
        _ => false,
    };
    let owner = ta.account.owner.to_bytes().to_vec();

    upsert_owner_for_token_account(
        db,
        mint.clone(),
        key_bytes,
        owner.clone(),
        delegate.clone(),
        account_update.slot() as i64,
        frozen,
        ta.account.amount,
        ta.account.delegated_amount as i64,
        spl_token_program,
    )
    .await?;

    // Metrics
    let mut token_owner_update = false;
    let mut token_delegate_update = false;
    let mut token_freeze_update = false;

    let txn = db.begin().await?;
    let asset_update = asset::Entity::find_by_id(mint.clone())
        .filter(
            asset::Column::OwnerType.eq("single").and(
                asset::Column::SlotUpdated
                    .is_null()
                    .or(asset::Column::SlotUpdated.lte(account_update.slot() as i64)),
            ),
        )
        .one(&txn)
        .await?;

    if let Some(asset) = asset_update {
        // Only handle token account updates for NFTs (supply=1)
        let asset_clone = asset.clone();
        if asset_clone.supply == 1 {
            let mut save_required = false;
            let mut active: asset::ActiveModel = asset.into();

            // Handle ownership updates
            let old_owner = asset_clone.owner.clone();
            let new_owner = owner.clone();
            if ta.account.amount > 0 && Some(new_owner) != old_owner {
                active.owner = Set(Some(owner.clone()));
                token_owner_update = true;
                save_required = true;
            }

            // Handle delegate updates
            if ta.account.amount > 0 && delegate.clone() != asset_clone.delegate {
                active.delegate = Set(delegate.clone());
                token_delegate_update = true;
                save_required = true;
            }

            // Handle freeze updates
            if ta.account.amount > 0 && frozen != asset_clone.frozen {
                active.frozen = Set(frozen);
                token_freeze_update = true;
                save_required = true;
            }

            let token_extensions = serde_json::to_value(ta.extensions.clone())
                .map_err(|e| IngesterError::SerializatonError(e.to_string()))?;

            if save_required {
                upsert_assets_token_account_columns(
                    AssetTokenAccountColumns {
                        mint,
                        owner: Some(owner),
                        frozen,
                        delegate,
                        slot_updated_token_account: Some(account_update.slot() as i64),
                    },
                    &txn,
                )
                .await?;
            }
        }
    }
    txn.commit().await?;

    // Publish metrics outside of the txn to reduce txn latency.
    if token_owner_update {
        metric! {
            statsd_count!("token_account.owner_update", 1);
        }
    }
    if token_delegate_update {
        metric! {
            statsd_count!("token_account.delegate_update", 1);
        }
    }
    if token_freeze_update {
        metric! {
            statsd_count!("token_account.freeze_update", 1);
        }
    }

    Ok(())
}
