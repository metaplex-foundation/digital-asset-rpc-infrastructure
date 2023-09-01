
use enum_iterator::{all, Sequence};
use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(Tasks::TaskStatus)
                    .values(vec![
                        TaskStatus::Pending,
                        TaskStatus::Running,
                        TaskStatus::Success,
                        TaskStatus::Failed,
                    ])
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Tasks::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Tasks::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Tasks::TaskType).string().not_null())
                    .col(ColumnDef::new(Tasks::Data).json_binary().not_null())
                    .col(
                        ColumnDef::new(Tasks::Status)
                            .enumeration(
                                Tasks::TaskStatus,
                                all::<TaskStatus>()
                                    .map(|e| e)
                                    .collect::<Vec<_>>(),
                            )
                            .not_null(),
                    )
                    .col(ColumnDef::new(Tasks::CreatedAt).date_time().not_null())
                    .col(ColumnDef::new(Tasks::LockedUntil).date_time().null())
                    .col(ColumnDef::new(Tasks::LockedBy).string().null())
                    .col(
                        ColumnDef::new(Tasks::MaxAttempts)
                            .small_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Tasks::Attempts)
                            .small_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Tasks::Duration).integer().null())
                    .col(ColumnDef::new(Tasks::Errors).text().null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Tasks::Table).to_owned())
            .await
    }
}
#[derive(Iden, Debug, PartialEq, Sequence)]
enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Iden)]
enum Tasks {
    TaskStatus,
    Table,
    Id,
    TaskType,
    Data,
    MaxAttempts,
    Attempts,
    Status,
    LockedUntil,
    LockedBy,
    CreatedAt,
    Duration,
    Errors,
}
