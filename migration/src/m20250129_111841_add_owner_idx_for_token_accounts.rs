use sea_orm_migration::prelude::*;

use crate::model::table::TokenAccounts;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_token_account_owner")
                    .col(TokenAccounts::Owner)
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_token_account_owner")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
