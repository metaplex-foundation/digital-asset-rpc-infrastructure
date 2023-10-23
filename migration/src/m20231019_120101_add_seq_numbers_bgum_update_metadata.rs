use digital_asset_types::dao::{asset, asset_creators, asset_data};
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
                ALTER TABLE asset_creators
                RENAME COLUMN seq to verified_seq;
                "
                .to_string(),
            ))
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(asset_data::Entity)
                    .add_column(ColumnDef::new(Alias::new("download_metadata_seq")).big_integer())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(asset::Entity)
                    .add_column(ColumnDef::new(Alias::new("update_metadata_seq")).big_integer())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "
                ALTER TABLE asset_creators
                RENAME COLUMN verified_seq to seq;
                "
                .to_string(),
            ))
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(asset_data::Entity)
                    .drop_column(Alias::new("download_metadata_seq"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(asset::Entity)
                    .drop_column(Alias::new("update_metadata_seq"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
