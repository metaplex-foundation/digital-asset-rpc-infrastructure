use sea_orm_migration::prelude::*;

use crate::model::table::AssetGrouping;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_grouping_id_value_verified_unique")
                    .col(AssetGrouping::AssetId)
                    .col(AssetGrouping::GroupValue)
                    .col(AssetGrouping::Verified)
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_id_value_verified_unique")
                    .table(AssetGrouping::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
