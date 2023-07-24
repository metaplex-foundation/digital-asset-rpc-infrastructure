use digital_asset_types::dao::asset_grouping;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .alter_table(
                Table::alter()
                    .table(asset_grouping::Entity)
                    .add_column(ColumnDef::new(Alias::new("group_info_seq")).big_integer())
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
                    .table(asset_grouping::Entity)
                    .drop_column(Alias::new("group_info_seq"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
