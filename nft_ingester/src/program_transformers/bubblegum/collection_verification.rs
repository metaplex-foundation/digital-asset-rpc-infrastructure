use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq,
    },
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use digital_asset_types::dao::asset_grouping;
use sea_orm::{entity::*, query::*, sea_query::OnConflict, DbBackend, Set};

pub async fn process<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    verify: bool,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl)) = (&parsing_result.leaf_update, &parsing_result.tree_update) {
        // Do we need to update the `slot_updated` field as well as part of the table
        // updates below?
        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
        #[allow(unreachable_patterns)]
        match le.schema {
            LeafSchema::V1 {
                id,
                owner,
                delegate,
                ..
            } => {
                let id_bytes = id.to_bytes();
                let owner_bytes = owner.to_bytes().to_vec();
                let delegate = if owner == delegate {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };

                // Partial update of asset table with just leaf.
                upsert_asset_with_leaf_info(
                    txn,
                    id_bytes.to_vec(),
                    Some(le.leaf_hash.to_vec()),
                    Some(seq as i64),
                    false,
                )
                .await?;

                // Partial update of asset table with just leaf owner and delegate.
                upsert_asset_with_owner_and_delegate_info(
                    txn,
                    id_bytes.to_vec(),
                    owner_bytes,
                    delegate,
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

                if verify {
                    if let Some(Payload::SetAndVerifyCollection { collection }) =
                        parsing_result.payload
                    {
                        let grouping = asset_grouping::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            group_key: Set("collection".to_string()),
                            group_value: Set(collection.to_string()),
                            seq: Set(seq as i64),
                            slot_updated: Set(bundle.slot as i64),
                            ..Default::default()
                        };
                        let mut query = asset_grouping::Entity::insert(grouping)
                            .on_conflict(
                                OnConflict::columns([
                                    asset_grouping::Column::AssetId,
                                    asset_grouping::Column::GroupKey,
                                ])
                                .update_columns([
                                    asset_grouping::Column::GroupKey,
                                    asset_grouping::Column::GroupValue,
                                    asset_grouping::Column::Seq,
                                    asset_grouping::Column::SlotUpdated,
                                ])
                                .to_owned(),
                            )
                            .build(DbBackend::Postgres);
                        query.sql = format!(
                    "{} WHERE excluded.slot_updated > asset_grouping.slot_updated AND excluded.seq >= asset_grouping.seq",
                    query.sql
                );
                        txn.execute(query).await?;
                    }
                }
                id_bytes
            }
            _ => return Err(IngesterError::NotImplemented),
        };

        return Ok(());
    };
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
