use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

use crate::model::table::AssetGrouping;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the existing unique constraint on (asset_id, group_key)
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_key_unique")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        // Create partial unique index for 'collection' group_key only
        // This maintains the constraint that each asset can only have one collection
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "CREATE UNIQUE INDEX IF NOT EXISTS asset_grouping_collection_unique \
                 ON asset_grouping (asset_id, group_key) \
                 WHERE group_key = 'collection'".to_string(),
            ))
            .await?;

        // Create unique constraint for non-collection group keys
        // This allows multiple groups per asset but prevents duplicate group values
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "CREATE UNIQUE INDEX IF NOT EXISTS asset_grouping_other_unique \
                 ON asset_grouping (asset_id, group_key, group_value) \
                 WHERE group_key != 'collection'".to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop the new partial indexes
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_collection_unique")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_other_unique")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        // Recreate the original unique constraint
        manager
            .create_index(
                sea_query::Index::create()
                    .unique()
                    .name("asset_grouping_key_unique")
                    .col(AssetGrouping::AssetId)
                    .col(AssetGrouping::GroupKey)
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}