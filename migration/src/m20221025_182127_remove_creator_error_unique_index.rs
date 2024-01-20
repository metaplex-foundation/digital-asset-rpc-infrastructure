use sea_orm_migration::prelude::*;

use crate::model::table::AssetCreators;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_creators_asset_id")
                    .table(AssetCreators::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        println!("Down migration not implemented");
        Ok(())
    }
}
