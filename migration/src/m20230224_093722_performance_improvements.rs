
use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE tokens SET (fillfactor = 95);".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE asset SET (fillfactor = 95);".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE asset SET (fillfactor = 95);".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS ta_delegate;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS asset_revision;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset DROP CONSTRAINT IF EXISTS asset_asset_data_fkey;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS asset_tree_leaf;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset_authority DROP CONSTRAINT IF EXISTS asset_authority_asset_id_fkey;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset_creators DROP CONSTRAINT IF EXISTS asset_creators_asset_id_fkey;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset_grouping DROP CONSTRAINT IF EXISTS asset_grouping_asset_id_fkey;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset_v1_account_attachments DROP CONSTRAINT IF EXISTS asset_v1_account_attachments_asset_id_fkey;
                ".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS asset_grouping_asset_id;
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
