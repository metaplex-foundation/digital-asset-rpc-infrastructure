use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DatabaseBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS backfill_items_failed_idx;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS backfill_items_locked_idx;
                "
                .to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS cl_items_tree_idx;
                "
                .to_string(),
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS backfill_items_slot_idx;;
                "
                .to_string(),
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS backfill_items_force_chk_idx;;
                "
                .to_string(),
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                DROP INDEX IF EXISTS backfill_items_backfilled_idx;;
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
