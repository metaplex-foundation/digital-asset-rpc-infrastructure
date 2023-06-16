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
                "
                ALTER TABLE asset
                ALTER COLUMN specification_version DROP NOT NULL,
                ALTER COLUMN specification_asset_class DROP NOT NULL,
                ALTER COLUMN seq DROP NOT NULL,
                ALTER COLUMN nonce DROP NOT NULL,
                ALTER COLUMN slot_updated DROP NOT NULL;
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset
                ALTER COLUMN specification_version SET NOT NULL,
                ALTER COLUMN specification_asset_class SET NOT NULL,
                ALTER COLUMN seq SET NOT NULL,
                ALTER COLUMN nonce SET NOT NULL,
                ALTER COLUMN slot_updated SET NOT NULL;
                "
                .to_string(),
            ))
            .await?;

        Ok(())
    }
}
