use digital_asset_types::dao::asset;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(asset::Entity)
                    .add_column(ColumnDef::new(Alias::new("leaf_seq")).big_integer())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(asset::Entity)
                    .drop_column(Alias::new("leaf_seq"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
