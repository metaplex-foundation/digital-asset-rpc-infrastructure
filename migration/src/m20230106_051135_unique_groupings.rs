use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_value")
                    .table(Alias::new("asset_grouping"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("asset_grouping_key_unique")
                    .col(Alias::new("asset_id"))
                    .col(Alias::new("group_key"))
                    .table(Alias::new("asset_grouping"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                sea_query::Index::drop()
                    .name("asset_grouping_key_unique")
                    .table(Alias::new("asset_grouping"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                sea_query::Index::create()
                    .name("asset_grouping_value")
                    .col(Alias::new("asset_id"))
                    .col(Alias::new("group_key"))
                    .table(Alias::new("asset_grouping"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
