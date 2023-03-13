use digital_asset_types::dao::tasks;
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
            CREATE INDEX tasks_created_at ON tasks USING BRIN(created_at);
            "
                .to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("tasks_created_at")
                    .table(tasks::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}