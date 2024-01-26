use sea_orm_migration::{prelude::*, sea_query::extension::postgres::Type};

use crate::model::table::ClAuditsV2;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_type(
                Type::alter()
                    .name(ClAuditsV2::Instruction)
                    .add_value(Alias::new("update_metadata"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // cannot rollback altering a type
        Ok(())
    }
}
