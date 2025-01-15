use {
    crate::error::{ProgramTransformerError, ProgramTransformerResult},
    blockbuster::token_metadata::{
        accounts::{DeprecatedMasterEditionV1, Edition, MasterEdition},
        types::Key,
    },
    digital_asset_types::dao::{
        asset_v1_account_attachments, sea_orm_active_enums::V1AccountAttachments,
    },
    sea_orm::{
        entity::{ActiveValue, EntityTrait},
        sea_query::{query::OnConflict, Alias, Condition, Expr},
        DatabaseTransaction,
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
    let ser = serde_json::to_value(me_data)
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;

    let model = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(id_bytes),
        attachment_type: ActiveValue::Set(version),
        data: ActiveValue::Set(Some(ser)),
        slot_updated: ActiveValue::Set(slot as i64),
        ..Default::default()
    };

    asset_v1_account_attachments::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .update_columns([
                    asset_v1_account_attachments::Column::AttachmentType,
                    asset_v1_account_attachments::Column::Data,
                    asset_v1_account_attachments::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_v1_account_attachments::Column::AttachmentType,
                                    )
                                    .ne(Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::AttachmentType,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_v1_account_attachments::Column::Data,
                                    )
                                    .ne(Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::Data,
                                    )),
                                ),
                        )
                        .add(
                            Expr::tbl(
                                asset_v1_account_attachments::Entity,
                                asset_v1_account_attachments::Column::SlotUpdated,
                            )
                            .lte(slot as i64),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(txn)
        .await?;

    Ok(())
}

pub async fn save_edition(
    id: Pubkey,
    slot: u64,
    e_data: &Edition,
    txn: &DatabaseTransaction,
) -> ProgramTransformerResult<()> {
    let id_bytes = id.to_bytes().to_vec();

    let ser = serde_json::to_value(e_data)
        .map_err(|e| ProgramTransformerError::SerializatonError(e.to_string()))?;

    let model = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(id_bytes),
        attachment_type: ActiveValue::Set(V1AccountAttachments::Edition),
        data: ActiveValue::Set(Some(ser)),
        slot_updated: ActiveValue::Set(slot as i64),
        ..Default::default()
    };

    asset_v1_account_attachments::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .update_columns([
                    asset_v1_account_attachments::Column::AttachmentType,
                    asset_v1_account_attachments::Column::Data,
                    asset_v1_account_attachments::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_v1_account_attachments::Column::AttachmentType,
                                    )
                                    .ne(Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::AttachmentType,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_v1_account_attachments::Column::Data,
                                    )
                                    .ne(Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::Data,
                                    )),
                                ),
                        )
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::SlotUpdated,
                                    )
                                    .lte(slot as i64),
                                )
                                .add(
                                    Expr::tbl(
                                        asset_v1_account_attachments::Entity,
                                        asset_v1_account_attachments::Column::SlotUpdated,
                                    )
                                    .is_null(),
                                ),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(txn)
        .await?;

    Ok(())
}
