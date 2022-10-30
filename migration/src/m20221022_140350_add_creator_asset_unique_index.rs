use digital_asset_types::dao::generated::asset_creators;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator")
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(asset_creators::Entity)
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
                    .col(asset_creators::Column::AssetId)
                    .col(asset_creators::Column::Creator)
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_creator_pos_unique")
                    .col(asset_creators::Column::AssetId)
                    .col(Alias::new("position"))
                    .table(asset_creators::Entity)
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
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creator_pos_unique")
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                sea_query::Index::create()
                    .name("asset_creator")
                    .col(asset_creators::Column::AssetId)
                    .col(asset_creators::Column::Creator)
                    .table(asset_creators::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Post {
    Table,
    Id,
    Title,
    Text,
}
