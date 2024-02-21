use {
    crate::error::{ProgramTransformerError, ProgramTransformerResult},
    blockbuster::token_metadata::{
        accounts::{DeprecatedMasterEditionV1, MasterEdition},
        types::Key,
    },
    digital_asset_types::dao::{
        asset, asset_v1_account_attachments, extensions,
        sea_orm_active_enums::{SpecificationAssetClass, V1AccountAttachments},
    },
    sea_orm::{
        entity::{ActiveModelTrait, ActiveValue, EntityTrait, RelationTrait},
        query::{JoinType, QuerySelect, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DatabaseTransaction, DbBackend,
    },
    solana_sdk::pubkey::Pubkey,
};

pub async fn save_v2_master_edition(
    id: Pubkey,
    slot: u64,
    me_data: &MasterEdition,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    save_master_edition(
        V1AccountAttachments::MasterEditionV2,
        id,
        slot,
        me_data,
        txn,
    )
    .await
}

pub async fn save_v1_master_edition(
    id: Pubkey,
    slot: u64,
    me_data: &DeprecatedMasterEditionV1,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    // This discards the deprecated `MasterEditionV1` fields
    // but sets the `Key`` as `MasterEditionV1`.
    let bridge = MasterEdition {
        supply: me_data.supply,
        max_supply: me_data.max_supply,
        key: Key::MasterEditionV1,
    };
    save_master_edition(
        V1AccountAttachments::MasterEditionV1,
        id,
        slot,
        &bridge,
        txn,
    )
    .await
}

pub async fn save_master_edition(
    version: V1AccountAttachments,
    id: Pubkey,
    slot: u64,
    me_data: &MasterEdition,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    let id_bytes = id.to_bytes().to_vec();
    let master_edition: Option<(asset_v1_account_attachments::Model, Option<asset::Model>)> =
        asset_v1_account_attachments::Entity::find_by_id(id.to_bytes().to_vec())
            .find_also_related(asset::Entity)
            .join(
                JoinType::InnerJoin,
                extensions::asset::Relation::AssetData.def(),
            )
            .one(txn)
            .await?;
    let ser = serde_json::to_value(me_data)
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;

    let model = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(id_bytes),
        attachment_type: ActiveValue::Set(version),
        data: ActiveValue::Set(Some(ser)),
        slot_updated: ActiveValue::Set(slot as i64),
        ..Default::default()
    };

    if let Some((_me, Some(asset))) = master_edition {
        let mut updatable: asset::ActiveModel = asset.into();
        updatable.supply = ActiveValue::Set(1);
        updatable.specification_asset_class = ActiveValue::Set(Some(SpecificationAssetClass::Nft));
        updatable.update(txn).await?;
    }

    let query = asset_v1_account_attachments::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .update_columns([
                    asset_v1_account_attachments::Column::AttachmentType,
                    asset_v1_account_attachments::Column::Data,
                    asset_v1_account_attachments::Column::SlotUpdated,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    txn.execute(query).await?;
    Ok(())
}
