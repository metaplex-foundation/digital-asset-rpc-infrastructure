use digital_asset_types::dao::asset_grouping;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_value")
                    .table(asset_grouping::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_grouping_key_unique")
                    .col(asset_grouping::Column::AssetId)
                    .col(asset_grouping::Column::GroupKey)
                    .table(asset_grouping::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_key_unique")
                    .table(asset_grouping::Entity)
                    .to_owned(),
            )
            .await?;
        
        manager
            .create_index(
                sea_query::Index::create()
                    .name("asset_grouping_value")
                    .col(asset_grouping::Column::AssetId)
                    .col(asset_grouping::Column::GroupKey)
                    .table(asset_grouping::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
