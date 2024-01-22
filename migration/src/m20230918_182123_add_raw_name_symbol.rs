use sea_orm_migration::prelude::*;

use crate::model::table::AssetData;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetData::Table)
                    .add_column(ColumnDef::new(AssetData::RawName).binary())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetData::Table)
                    .add_column(ColumnDef::new(AssetData::RawSymbol).binary())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetData::Table)
                    .drop_column(AssetData::RawName)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetData::Table)
                    .drop_column(AssetData::RawSymbol)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[allow(dead_code)]
#[derive(Iden)]
enum Post {
    Table,
    Id,
    Title,
    Text,
}
