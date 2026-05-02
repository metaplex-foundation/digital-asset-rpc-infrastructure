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
        // Partial unique index for the NULL sentinel row used when an asset
        // has no group relationships.  This allows ON CONFLICT to target a
        // single NULL-value row per (asset_id, 'group') combination.
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "CREATE UNIQUE INDEX IF NOT EXISTS asset_grouping_group_null_unique \
                 ON asset_grouping (asset_id, group_key) \
                 WHERE group_key = 'group' AND group_value IS NULL"
                    .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_group_null_unique")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
