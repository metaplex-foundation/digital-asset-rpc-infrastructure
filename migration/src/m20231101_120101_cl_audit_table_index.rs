use digital_asset_types::dao::cl_audits;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_cl_audits_tree")
                    .col(cl_audits::Column::Tree)
                    .table(cl_audits::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_cl_audits_leaf_id")
                    .col(cl_audits::Column::LeafIdx)
                    .table(cl_audits::Entity)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_cl_audits_tree")
                    .table(cl_audits::Entity)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_cl_audits_leaf_id")
                    .table(cl_audits::Entity)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
