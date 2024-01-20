use sea_orm_migration::prelude::*;

use crate::model::table::AssetGrouping;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .alter_table(
                Table::alter()
                    .table(AssetGrouping::Table)
                    .add_column(ColumnDef::new(AssetGrouping::GroupInfoSeq).big_integer())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .alter_table(
                Table::alter()
                    .table(AssetGrouping::Table)
                    .drop_column(AssetGrouping::GroupInfoSeq)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
