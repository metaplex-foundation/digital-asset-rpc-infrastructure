#[derive(DeriveMigrationName)]
pub struct Migration;
use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
        .get_connection()
        .execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX IF NOT EXISTS character_history_character_id on character_history (character_id);".to_string(),
        ))
        .await?;
        manager
            .create_table(
                Table::create()
                    .table(CharacterHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CharacterHistory::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CharacterHistory::CharacterId)
                            .binary()
                            .not_null(),
                    )
                    .col(ColumnDef::new(CharacterHistory::Event).string().not_null())
                    .col(
                        ColumnDef::new(CharacterHistory::EventData)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CharacterHistory::CreatedAt)
                            .date_time()
                            .default(SimpleExpr::Keyword(Keyword::CurrentTimestamp))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CharacterHistory::SlotUpdated)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("character_history_character_id")
                    .table(CharacterHistory::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(CharacterHistory::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum CharacterHistory {
    Table,
    Id,
    CharacterId,
    Event,
    EventData,
    CreatedAt,
    SlotUpdated,
}
