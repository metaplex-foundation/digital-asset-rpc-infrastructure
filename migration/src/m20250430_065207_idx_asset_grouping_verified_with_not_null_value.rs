use super::model::table::TokenAccounts;
use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_grouping_verified_with_not_null_value
ON asset_grouping(asset_id, verified)
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
                    .name("idx_asset_grouping_verified_with_not_null_value")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
