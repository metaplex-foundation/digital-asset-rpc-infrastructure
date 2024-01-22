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
                    .add_column(ColumnDef::new(Asset::DataHash).string().char_len(50))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::CreatorHash).string().char_len(50))
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
                    .drop_column(Asset::DataHash)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::CreatorHash)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
