use sea_orm_migration::prelude::*;

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .modify_column(
                        ColumnDef::new(Asset::OwnerType)
                            .string()
                            .not_null()
                            .default("Unknown"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .modify_column(
                        ColumnDef::new(Asset::OwnerType)
                            .string()
                            .not_null()
                            .default("Single"),
                    )
                    .to_owned(),
            )
            .await
    }
}
