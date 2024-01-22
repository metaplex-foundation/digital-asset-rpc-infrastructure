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
                    .add_column(ColumnDef::new(Alias::new("collection")).binary())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("collection_verified"))
                            .boolean()
                            .default(false)
                            .not_null(),
                    )
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
                    .drop_column(Alias::new("collection"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(Asset::Table)
                    .drop_column(Alias::new("collection_verified"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
