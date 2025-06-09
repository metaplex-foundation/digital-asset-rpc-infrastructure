use sea_orm::{ConnectionTrait, DatabaseBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::model::table::TokenAccounts;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            r#"
               CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ta_delegate
               ON token_accounts USING hash (delegate);
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
                    .name("idx_ta_delegate")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await
    }
}
