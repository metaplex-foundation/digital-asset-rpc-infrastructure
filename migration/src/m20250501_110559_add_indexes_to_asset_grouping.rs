use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::model::table::AssetGrouping;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_grouping_collection_verified
ON asset_grouping (group_key, group_value, verified, asset_id)
WHERE group_value IS NOT NULL;
"
            .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_grouping_collection_verified")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
