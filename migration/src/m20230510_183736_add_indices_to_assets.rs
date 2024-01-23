use sea_orm_migration::prelude::*;

use crate::model::table::{AssetAuthority, AssetCreators};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_asset_creators_asset_id")
                    .col(AssetCreators::AssetId)
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_asset_creators_creator")
                    .col(AssetCreators::Creator)
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_asset_authority_authority")
                    .col(AssetAuthority::Authority)
                    .table(AssetAuthority::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_creators_asset_id")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_creators_creator")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_authority_authority")
                    .table(AssetAuthority::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
