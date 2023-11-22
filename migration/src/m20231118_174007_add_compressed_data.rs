use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(CompressedData::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CompressedData::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(CompressedData::TreeId).binary().not_null())
                    .col(ColumnDef::new(CompressedData::LeafIdx).big_integer().not_null())
                    .col(ColumnDef::new(CompressedData::Seq).big_integer().not_null())
                    .col(ColumnDef::new(CompressedData::RawData).binary().not_null())
                    .col(ColumnDef::new(CompressedData::ParsedData).json_binary())
                    .col(ColumnDef::new(CompressedData::Program).binary())
                    .col(ColumnDef::new(CompressedData::CreatedAt).date_time().default(SimpleExpr::Keyword(Keyword::CurrentTimestamp)).not_null())
                    .col(ColumnDef::new(CompressedData::SlotUpdated).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CompressedData::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum CompressedData {
    Table,
    Id,
    TreeId,
    LeafIdx,
    Seq,
    RawData,
    ParsedData,
    Program,
    CreatedAt,
    SlotUpdated
}
