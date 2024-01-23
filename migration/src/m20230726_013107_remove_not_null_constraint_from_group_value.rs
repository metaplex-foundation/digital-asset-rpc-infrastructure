use sea_orm_migration::prelude::*;

use crate::model::table::AssetGrouping;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetGrouping::Table)
                    .modify_column(ColumnDef::new(AssetGrouping::GroupValue).null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    // If the `group_value` table already has some NULL values, this rollback won't work.
    // Thus, for rollback, you'll first need to delete all the rows with NULL `group_value`, and then run the rollback.
    // Query: `DELETE FROM asset_grouping WHERE group_value IS NULL;`
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                sea_query::Table::alter()
                    .table(AssetGrouping::Table)
                    .modify_column(ColumnDef::new(AssetGrouping::GroupValue).not_null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
