use sea_orm_migration::prelude::*;

use crate::model::table::MplCore;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MplCore::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MplCore::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(MplCore::AssetId).binary().not_null())
                    .col(ColumnDef::new(MplCore::Seq).big_integer())
                    .col(ColumnDef::new(MplCore::SlotUpdated).big_integer())
                    .col(ColumnDef::new(MplCore::Plugins).json_binary())
                    .col(ColumnDef::new(MplCore::UnknownPlugins).json_binary())
                    .col(ColumnDef::new(MplCore::PluginsJsonVersion).unsigned())
                    .col(ColumnDef::new(MplCore::CollectionNumMinted).unsigned())
                    .col(ColumnDef::new(MplCore::CollectionCurrentSize).unsigned())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("mpl_core_asset_id")
                    .col(MplCore::AssetId)
                    .table(MplCore::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MplCore::Table).to_owned())
            .await?;
        Ok(())
    }
}
