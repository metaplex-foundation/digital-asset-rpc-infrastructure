use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

use crate::model::table::TokenAccounts;
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("ta_amount")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("ta_amount_del")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("ta_slot_updated_idx")
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE token_accounts SET (fillfactor = 70);".to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "ALTER TABLE token_accounts SET (fillfactor = 90);".to_string(),
            ))
            .await?;
        manager
            .create_index(
                sea_query::Index::create()
                    .name("ta_amount")
                    .index_type(sea_query::IndexType::BTree)
                    .col(TokenAccounts::Amount)
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                sea_query::Index::create()
                    .name("ta_amount_del")
                    .index_type(sea_query::IndexType::BTree)
                    .col(TokenAccounts::DelegatedAmount)
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                sea_query::Index::create()
                    .name("ta_slot_updated_idx")
                    .index_type(sea_query::IndexType::BTree)
                    .table(TokenAccounts::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
<<<<<<< HEAD
=======

/// Learn more at https://docs.rs/sea-query#iden
#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
#[derive(Iden)]
enum Index {
    BRIN,
}
>>>>>>> bb2eb9c (include migration to workspace)
