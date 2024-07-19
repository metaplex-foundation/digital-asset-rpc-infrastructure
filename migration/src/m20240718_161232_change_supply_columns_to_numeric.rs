use crate::model::table::{Asset, Tokens};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .modify_column(ColumnDef::new(Asset::Supply).decimal_len(20, 0).not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tokens::Table)
                    .modify_column(ColumnDef::new(Tokens::Supply).decimal_len(20, 0).not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .modify_column(ColumnDef::new(Asset::Supply).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Tokens::Table)
                    .modify_column(ColumnDef::new(Tokens::Supply).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }
}
