use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

use crate::model::table::{AssetData, Tokens};
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("t_slot_updated_idx")
                    .table(Tokens::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("t_supply")
                    .table(Tokens::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("t_decimals")
                    .table(Tokens::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE tokens SET (fillfactor = 70);".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE asset SET (fillfactor = 85);".to_string(),
            ))
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE asset SET (fillfactor = 85);".to_string(),
            ))
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("slot_updated_idx")
                    .table(AssetData::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
