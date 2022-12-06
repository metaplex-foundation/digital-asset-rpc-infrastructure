use digital_asset_types::dao::backfill_items;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_failed_idx")
                    .col(backfill_items::Column::Failed)
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_tree_failed_idx")
                    .col(backfill_items::Column::Tree)
                    .col(backfill_items::Column::Failed)
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_locked_idx")
                    .col(backfill_items::Column::Locked)
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_tree_locked_idx")
                    .col(backfill_items::Column::Tree)
                    .col(backfill_items::Column::Locked)
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_failed_idx")
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_tree_failed_idx")
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_locked_idx")
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_tree_locked_idx")
                    .table(backfill_items::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
