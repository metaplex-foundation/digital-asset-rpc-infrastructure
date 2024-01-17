use digital_asset_types::dao::asset_creators;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_unique")
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_creator_unique")
                    .col(asset_creators::Column::AssetId)
                    .col(asset_creators::Column::Creator)
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
