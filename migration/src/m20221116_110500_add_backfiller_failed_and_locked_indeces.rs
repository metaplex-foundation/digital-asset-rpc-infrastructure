use sea_orm_migration::prelude::*;

use crate::model::table::BackfillItems;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_failed_idx")
                    .col(BackfillItems::Failed)
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_tree_failed_idx")
                    .col(BackfillItems::Tree)
                    .col(BackfillItems::Failed)
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_locked_idx")
                    .col(BackfillItems::Locked)
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("backfill_items_tree_locked_idx")
                    .col(BackfillItems::Tree)
                    .col(BackfillItems::Locked)
                    .table(BackfillItems::Table)
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
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_tree_failed_idx")
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_locked_idx")
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("backfill_items_tree_locked_idx")
                    .table(BackfillItems::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
