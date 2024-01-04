use sea_orm::Statement;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DatabaseBackend};

use crate::m20230919_072154_cl_audits::ClAudits;

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
                DROP TABLE IF EXISTS cl_audits;
                "
                .to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ClAudits::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ClAudits::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment(),
                    )
                    .col(ColumnDef::new(ClAudits::Tree).binary().not_null())
                    .col(ColumnDef::new(ClAudits::NodeIdx).big_integer().not_null())
                    .col(ColumnDef::new(ClAudits::LeafIdx).big_integer())
                    .col(ColumnDef::new(ClAudits::Seq).big_integer().not_null())
                    .col(ColumnDef::new(ClAudits::Level).big_integer().not_null())
                    .col(ColumnDef::new(ClAudits::Hash).binary().not_null())
                    .col(
                        ColumnDef::new(ClAudits::CreatedAt)
                            .date_time()
                            .default(SimpleExpr::Keyword(Keyword::CurrentTimestamp))
                            .not_null(),
                    )
                    .col(ColumnDef::new(ClAudits::Tx).string().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
