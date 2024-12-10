use sea_orm_migration::prelude::*;

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_asset_owner_supply")
                    .col(Asset::Owner)
                    .col(Asset::Supply)
                    .col(Asset::Burnt)
                    .col(Asset::OwnerType)
                    .table(Asset::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_asset_owner_supply")
                    .table(Asset::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
