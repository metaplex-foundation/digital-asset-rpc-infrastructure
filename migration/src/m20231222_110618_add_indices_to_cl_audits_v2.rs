use digital_asset_types::dao::cl_audits_v2;
use sea_orm_migration::prelude::*;
use sea_orm::{ConnectionTrait, Statement, DatabaseBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // manager
        //     .create_index(
        //         Index::create()
        //             .name("tree_idx")
        //             .table(cl_audits_v2::Entity)
        //             .col(cl_audits_v2::Column::Tree)
        //             .to_owned(),
        //     )
        //     .await?;

            conn.execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "CREATE INDEX tree_seq_idx ON cl_audits_v2 (tree, seq);".to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("tree_idx")
                    .table(cl_audits_v2::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("tree_seq_idx")
                    .table(cl_audits_v2::Entity)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
