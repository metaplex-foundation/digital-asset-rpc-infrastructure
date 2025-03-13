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
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS ta_owner_amount ON token_accounts (owner,amount);"
                .to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS ta_mint_amount ON token_accounts (mint,amount);"
                .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("ta_owner_amount")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("ta_mint_amount")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
