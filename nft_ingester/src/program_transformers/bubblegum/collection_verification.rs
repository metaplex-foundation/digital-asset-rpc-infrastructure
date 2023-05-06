use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
};
use digital_asset_types::dao::{asset, asset_grouping};
use sea_orm::{entity::*, query::*, sea_query::OnConflict, DbBackend, Set, Unchanged};

use super::{save_changelog_event, update_asset};
use crate::error::IngesterError;
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
        match le.schema {
            LeafSchema::V1 { id, .. } => {
                let id_bytes = id.to_bytes().to_vec();

                let asset_to_update = asset::ActiveModel {
                    id: Unchanged(id_bytes.clone()),
                    leaf: Set(Some(le.leaf_hash.to_vec())),
                    seq: Set(seq as i64),
                    ..Default::default()
                };
                update_asset(txn, id_bytes.clone(), Some(seq), asset_to_update).await?;

                if verify {
                    if let Some(Payload::SetAndVerifyCollection { collection }) =
                        parsing_result.payload
                    {
                        let grouping = asset_grouping::ActiveModel {
                            asset_id: Set(id_bytes.clone()),
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
            } // _ => return Err(IngesterError::NotImplemented),
        };

        return Ok(());
    };
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
