use sea_orm_migration::prelude::*;

use crate::model::table::AssetCreators;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(AssetCreators::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("position"))
                            .small_integer()
                            .not_null()
                            .default(-1),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_creator_unique")
                    .col(AssetCreators::AssetId)
                    .col(AssetCreators::Creator)
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_creator_pos_unique")
                    .col(AssetCreators::AssetId)
                    .col(Alias::new("position"))
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_unique")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_pos_unique")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                sea_query::Index::create()
                    .name("asset_creator")
                    .col(AssetCreators::AssetId)
                    .col(AssetCreators::Creator)
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
