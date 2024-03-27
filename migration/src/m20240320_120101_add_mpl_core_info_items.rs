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
                    .add_column(
                        ColumnDef::new(Asset::MplCoreCollectionNumMinted)
                            .integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::MplCoreCollectionCurrentSize)
                            .integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::MplCorePluginsJsonVersion)
                            .integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::MplCoreCollectionNumMinted)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::MplCoreCollectionCurrentSize)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::MplCorePluginsJsonVersion)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
