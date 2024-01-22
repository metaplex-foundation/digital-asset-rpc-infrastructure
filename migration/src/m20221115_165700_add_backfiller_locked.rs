use sea_orm_migration::prelude::*;

use crate::model::table::BackfillItems;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .alter_table(
                Table::alter()
                    .table(BackfillItems::Table)
                    .add_column(
                        ColumnDef::new(BackfillItems::Locked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts
        manager
            .alter_table(
                Table::alter()
                    .table(BackfillItems::Table)
                    .drop_column(BackfillItems::Locked)
                    .to_owned(),
            )
            .await
    }
}
