use sea_orm_migration::prelude::*;

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::AssetDataHash).string().char_len(50))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::BubblegumFlags).small_integer())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::NonTransferable).boolean())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::AssetDataHash)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::BubblegumFlags)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::NonTransferable)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
