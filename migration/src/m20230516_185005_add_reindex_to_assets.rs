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
                    .add_column(ColumnDef::new(AssetData::Reindex).boolean().default(false))
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
                    .drop_column(AssetData::Reindex)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
