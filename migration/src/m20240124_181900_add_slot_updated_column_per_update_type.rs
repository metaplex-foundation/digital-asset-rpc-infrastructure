use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            ADD COLUMN slot_updated_metadata_account int8;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            ADD COLUMN slot_updated_token_account int8;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            ADD COLUMN slot_updated_mint_account int8;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            ADD COLUMN slot_updated_cnft_transaction int8;
            "
                .to_string(),
            ))
            .await?;

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
        let connection = manager.get_connection();

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            DROP COLUMN IF EXISTS slot_updated_metadata_account;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            DROP COLUMN IF EXISTS slot_updated_token_account;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            DROP COLUMN IF EXISTS slot_updated_mint_account;
            "
                .to_string(),
            ))
            .await?;

        connection
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
            ALTER TABLE asset
            DROP COLUMN IF EXISTS slot_updated_cnft_transaction;
            "
                .to_string(),
            ))
            .await?;

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
