use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CompressedDataChangelog::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CompressedDataChangelog::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CompressedDataChangelog::CompressedDataId)
                            .binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CompressedDataChangelog::Key).text().null())
                    .col(
                        ColumnDef::new(CompressedDataChangelog::Data)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CompressedDataChangelog::Seq)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CompressedDataChangelog::CreatedAt)
                            .date_time()
                            .default(SimpleExpr::Keyword(Keyword::CurrentTimestamp))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CompressedDataChangelog::SlotUpdated)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(CompressedDataChangelog::Table)
                    .to_owned(),
            )
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum CompressedDataChangelog {
    Table,
    Id,
    CompressedDataId,
    Key,
    Data,
    Seq,
    CreatedAt,
    SlotUpdated,
}
