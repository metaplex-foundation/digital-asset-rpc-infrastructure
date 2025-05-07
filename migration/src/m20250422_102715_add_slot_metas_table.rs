use crate::model::table::SlotMeta;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SlotMeta::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SlotMeta::Slot)
                            .big_integer()
                            .primary_key()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_slot_desc")
                    .table(SlotMeta::Table)
                    .col((SlotMeta::Slot, IndexOrder::Desc))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name("idx_slot_desc").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(SlotMeta::Table).to_owned())
            .await
    }
}
