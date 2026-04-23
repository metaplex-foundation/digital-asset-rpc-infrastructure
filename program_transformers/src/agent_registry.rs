use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        find_model_with_retry,
    },
    blockbuster::programs::agent_registry::AgentRegistryAccount,
    digital_asset_types::dao::asset,
    sea_orm::{
        entity::{ColumnTrait, EntityTrait},
        sea_query::Expr,
        ConnectionTrait, QueryFilter, TransactionTrait,
    },
    solana_sdk::pubkey::Pubkey,
};

const RETRY_INTERVALS: &[u64] = &[0, 5, 10];

/// Handle an Agent Registry program account update.
///
/// Writes `agent_token` (and its slot guard) directly on the `asset` row.
/// Uses a plain UPDATE rather than an INSERT ON CONFLICT upsert because the
/// Agent Registry is a secondary program that decorates existing Core assets
/// — it should never create an asset row. The `find_model_with_retry` above
/// validates the asset exists and isn't burnt before we reach the write.
pub async fn handle_agent_registry_account<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    _account_pubkey: Pubkey,
    parsed: &AgentRegistryAccount,
    slot: u64,
) -> ProgramTransformerResult<()> {
    let Some(inner) = parsed.inner.as_ref() else {
        return Ok(());
    };

    let asset_id = inner.asset.to_bytes().to_vec();

    let asset_model = find_model_with_retry(
        conn,
        "asset",
        &asset::Entity::find_by_id(asset_id.clone()),
        RETRY_INTERVALS,
    )
    .await?;

    let Some(asset_model) = asset_model else {
        return Ok(());
    };
    if asset_model.burnt {
        return Ok(());
    }

    let slot_i = slot as i64;
    let agent_token_bytes: Option<Vec<u8>> =
        inner.agent_token_mint.map(|mint| mint.to_bytes().to_vec());

    asset::Entity::update_many()
        .col_expr(asset::Column::AgentToken, Expr::value(agent_token_bytes))
        .col_expr(asset::Column::SlotUpdatedAgentRegistry, Expr::value(slot_i))
        .filter(asset::Column::Id.eq(asset_id))
        .filter(
            asset::Column::SlotUpdatedAgentRegistry
                .is_null()
                .or(asset::Column::SlotUpdatedAgentRegistry.lte(slot_i)),
        )
        .exec(conn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}
