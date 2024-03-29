use sea_orm_migration::prelude::*;

use crate::model::table::{Asset, TokenAccounts, Tokens};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::MintExtensions).json_binary())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tokens::Table)
                    .add_column(ColumnDef::new(Tokens::Extensions).json_binary())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TokenAccounts::Table)
                    .add_column(ColumnDef::new(TokenAccounts::Extensions).json_binary())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::MintExtensions)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tokens::Table)
                    .drop_column(Tokens::Extensions)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TokenAccounts::Table)
                    .drop_column(TokenAccounts::Extensions)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
