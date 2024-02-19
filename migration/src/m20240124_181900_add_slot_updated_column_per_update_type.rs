use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::SlotUpdatedMetadataAccount)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::SlotUpdatedTokenAccount)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::SlotUpdatedMintAccount)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::SlotUpdatedCnftTransaction)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        let connection = manager.get_connection();
        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                CREATE OR REPLACE FUNCTION update_slot_updated()
                RETURNS TRIGGER AS $$
                BEGIN
                   NEW.slot_updated = GREATEST(NEW.slot_updated_token_account, NEW.slot_updated_mint_account, NEW.slot_updated_metadata_account, NEW.slot_updated_cnft_transaction);
                   RETURN NEW;
                END;
                $$ language 'plpgsql';
                "
                    .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                CREATE TRIGGER update_slot_updated_trigger
                BEFORE UPDATE ON asset
                FOR EACH ROW
                EXECUTE PROCEDURE update_slot_updated();
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::SlotUpdatedMetadataAccount)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::SlotUpdatedTokenAccount)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::SlotUpdatedMintAccount)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::SlotUpdatedCnftTransaction)
                    .to_owned(),
            )
            .await?;

        let connection = manager.get_connection();

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP TRIGGER IF EXISTS update_slot_updated_trigger ON asset;
                "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP FUNCTION IF EXISTS update_slot_updated();
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }
}
