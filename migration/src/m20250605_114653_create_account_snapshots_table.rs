use sea_orm_migration::prelude::*;

use crate::model::table::AccountSnapshot;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AccountSnapshot::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AccountSnapshot::Pubkey)
                            .binary()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AccountSnapshot::Slot)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AccountSnapshot::Owner).binary().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("account_snapshots_owner")
                    .index_type(sea_query::IndexType::Hash)
                    .col(AccountSnapshot::Owner)
                    .table(AccountSnapshot::Table)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AccountSnapshot::Table).to_owned())
            .await
    }
}
