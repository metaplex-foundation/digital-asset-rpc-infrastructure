use digital_asset_types::dao::{asset_authority, asset_creators};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_asset_creators_asset_id")
                    .col(asset_creators::Column::AssetId)
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_asset_creators_creator")
                    .col(asset_creators::Column::Creator)
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_asset_authority_authority")
                    .col(asset_authority::Column::Authority)
                    .table(asset_authority::Entity)
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
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_creators_creator")
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_authority_authority")
                    .table(asset_authority::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
