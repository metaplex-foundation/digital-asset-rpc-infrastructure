use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, DatabaseBackend, Statement},
};

use crate::model::table::Asset;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::IsAgent)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::AgentToken).binary().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(ColumnDef::new(Asset::AssetSigner).binary().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .add_column(
                        ColumnDef::new(Asset::SlotUpdatedAgentRegistry)
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_is_agent \
             ON asset (id) WHERE is_agent = TRUE"
                .to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_agent_token \
             ON asset (agent_token) WHERE agent_token IS NOT NULL"
                .to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_asset_asset_signer \
             ON asset (asset_signer) WHERE asset_signer IS NOT NULL"
                .to_string(),
        ))
        .await?;

        // Update the trigger that computes asset.slot_updated from per-source
        // slot columns to include the new agent registry source.
        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE OR REPLACE FUNCTION update_slot_updated() \
             RETURNS TRIGGER AS $$ \
             BEGIN \
                NEW.slot_updated = GREATEST( \
                    NEW.slot_updated_token_account, \
                    NEW.slot_updated_mint_account, \
                    NEW.slot_updated_metadata_account, \
                    NEW.slot_updated_cnft_transaction, \
                    NEW.slot_updated_agent_registry \
                ); \
                RETURN NEW; \
             END; \
             $$ language 'plpgsql';"
                .to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Restore trigger without agent_registry column.
        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "CREATE OR REPLACE FUNCTION update_slot_updated() \
             RETURNS TRIGGER AS $$ \
             BEGIN \
                NEW.slot_updated = GREATEST( \
                    NEW.slot_updated_token_account, \
                    NEW.slot_updated_mint_account, \
                    NEW.slot_updated_metadata_account, \
                    NEW.slot_updated_cnft_transaction \
                ); \
                RETURN NEW; \
             END; \
             $$ language 'plpgsql';"
                .to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "DROP INDEX IF EXISTS idx_asset_asset_signer".to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "DROP INDEX IF EXISTS idx_asset_agent_token".to_string(),
        ))
        .await?;

        conn.execute(Statement::from_string(
            DatabaseBackend::Postgres,
            "DROP INDEX IF EXISTS idx_asset_is_agent".to_string(),
        ))
        .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::SlotUpdatedAgentRegistry)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::AssetSigner)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::AgentToken)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Asset::Table)
                    .drop_column(Asset::IsAgent)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
