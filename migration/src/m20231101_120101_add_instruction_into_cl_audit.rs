use digital_asset_types::dao::cl_audits;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(cl_audits::Entity)
                    .add_column(ColumnDef::new(Alias::new("Instruction")).string())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(cl_audits::Entity)
                    .drop_column(Alias::new("Instruction"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}