use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            r#"
                CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_supply_burnt_owner_type
                ON asset (supply, burnt, owner_type);
                "#
            .to_owned(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_supply_burnt_owner_type")
                    .table(Asset::Table)
                    .to_owned(),
            )
            .await
    }
}
