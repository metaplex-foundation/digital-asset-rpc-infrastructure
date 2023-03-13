use sea_orm_migration::prelude::*;
use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {

        let conn = manager.get_connection();
        let sql = r"
            CREATE TABLE IF NOT EXISTS raw_account_updates (
                id SERIAL PRIMARY KEY,
                pubkey bytea NOT NULL,
                data bytea NOT NULL,
                slot bigint NOT NULL,
                slot_index int NOT NULL,
                lamports bigint NOT NULL,
                owner bytea NOT NULL,
            );
            CREATE INDEX IF NOT EXISTS raw_account_updates_owner ON raw_account_updates (owner);
        ";


        let sqls: Vec<&str> = sql.split("-- @@@@@@").collect();
        for sqlst in sqls {
            let stmt = Statement::from_string(manager.get_database_backend(), sqlst.to_string());
            conn.execute(stmt).await.map(|_| ())?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(RawAccountUpdates::Table).to_owned())
            .await
    }
}

