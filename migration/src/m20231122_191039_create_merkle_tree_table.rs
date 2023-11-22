use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MerkleTree::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MerkleTree::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(MerkleTree::DataSchema).binary().not_null())
                    .col(ColumnDef::new(MerkleTree::CreatedAt).date_time().default(SimpleExpr::Keyword(Keyword::CurrentTimestamp)).not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MerkleTree::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum MerkleTree {
    Table,
    Id,
    DataSchema,
    CreatedAt,
}
