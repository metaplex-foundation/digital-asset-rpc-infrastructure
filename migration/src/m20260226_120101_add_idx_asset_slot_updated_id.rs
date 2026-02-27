use super::model::table::Asset;
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
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_slot_updated_id ON asset (slot_updated ASC, id ASC);"
                .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_slot_updated_id")
                    .table(Asset::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
