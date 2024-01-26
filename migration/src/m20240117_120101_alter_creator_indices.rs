use sea_orm_migration::prelude::*;

use crate::model::table::AssetCreators;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // New index to serve as a replacement for "asset_creator_unique" when someone queries GetAssetsByCreator.
        // We add it before deleting the "asset_creator_unique" index to ensure no customer impact.
        manager
            .create_index(
                Index::create()
                    .name("asset_creator_verified")
                    .table(AssetCreators::Table)
                    .col(AssetCreators::AssetId)
                    .col(AssetCreators::Creator)
                    .col(AssetCreators::Verified)
                    .to_owned(),
            )
            .await?;

        // We no longer want to enforce uniques on the (asset_id, creator) pairs.
        // We may end up with duplicate (asset_id, creator) pairs during indexing, because a creator can change position.
        // Any stale rows (older seq/slot_updated) will be ignored, meaning the API users will never see duplicate creators.
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_unique")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;

        // This index is unused and can be removed.
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_verified_creator")
                    .table(AssetCreators::Table)
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
                    .table(AssetCreators::Table)
                    .col(AssetCreators::AssetId)
                    .col(AssetCreators::Creator)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("asset_verified_creator")
                    .table(AssetCreators::Table)
                    .col(AssetCreators::AssetId)
                    .col(AssetCreators::Verified)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_verified")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
