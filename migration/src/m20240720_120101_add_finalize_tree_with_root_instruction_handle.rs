use crate::sea_orm::{DatabaseBackend, Statement};
use enum_iterator::all;
use enum_iterator_derive::Sequence;
use sea_orm::sea_query::extension::postgres::Type;
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(RollupToVerify::RollupFailStatus)
                    .values([
                        FailedRollupState::ChecksumVerifyFailed,
                        FailedRollupState::RollupVerifyFailed,
                        FailedRollupState::DownloadFailed,
                        FailedRollupState::FileSerialization,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(RollupToVerify::RollupPersistingState)
                    .values([
                        PersistingRollupState::ReceivedTransaction,
                        PersistingRollupState::StartProcessing,
                        PersistingRollupState::FailedToPersist,
                        PersistingRollupState::SuccessfullyDownload,
                        PersistingRollupState::SuccessfullyValidate,
                        PersistingRollupState::StoredUpdate,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RollupToVerify::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RollupToVerify::FileHash)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RollupToVerify::Url).string().not_null())
                    .col(
                        ColumnDef::new(RollupToVerify::CreatedAtSlot)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RollupToVerify::Signature)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(RollupToVerify::Staker).binary().not_null())
                    .col(
                        ColumnDef::new(RollupToVerify::DownloadAttempts)
                            .unsigned()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RollupToVerify::RollupPersistingState)
                            .enumeration(
                                RollupToVerify::RollupPersistingState,
                                all::<PersistingRollupState>().collect::<Vec<_>>(),
                            )
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RollupToVerify::RollupFailStatus)
                            .enumeration(
                                RollupToVerify::RollupFailStatus,
                                all::<FailedRollupState>().collect::<Vec<_>>(),
                            )
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_created_at_slot")
                    .table(RollupToVerify::Table)
                    .col(RollupToVerify::CreatedAtSlot)
                    .col(RollupToVerify::RollupPersistingState)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Rollup::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Rollup::FileHash)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Rollup::RollupBinaryBincode)
                            .binary()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "CREATE FUNCTION notify_new_rollup() RETURNS trigger LANGUAGE plpgsql AS $$
                    BEGIN
                      PERFORM pg_notify('new_rollup', NEW::text);
                      RETURN NEW;
                    END;
                    $$;
                    CREATE TRIGGER rollup_to_verify_trigger
                    AFTER INSERT ON rollup_to_verify
                    FOR EACH ROW EXECUTE FUNCTION notify_new_rollup();"
                    .to_string(),
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute(Statement::from_string(
                DatabaseBackend::Postgres,
                "DROP TRIGGER IF EXISTS rollup_to_verify_trigger ON rollup_to_verify;
                DROP FUNCTION IF EXISTS notify_new_rollup;"
                    .to_string(),
            ))
            .await?;
        manager
            .drop_table(Table::drop().table(RollupToVerify::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Rollup::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum RollupToVerify {
    Table,
    Url,
    FileHash,
    CreatedAtSlot,
    Signature,
    DownloadAttempts,
    RollupPersistingState,
    RollupFailStatus,
    Staker,
}

#[derive(Iden, Debug, PartialEq, Sequence)]
enum PersistingRollupState {
    ReceivedTransaction,
    FailedToPersist,
    StartProcessing,
    SuccessfullyDownload,
    SuccessfullyValidate,
    StoredUpdate,
}

#[derive(Iden, Debug, PartialEq, Sequence)]
enum FailedRollupState {
    DownloadFailed,
    ChecksumVerifyFailed,
    RollupVerifyFailed,
    FileSerialization,
}

#[derive(Iden)]
enum Rollup {
    Table,
    FileHash,
    RollupBinaryBincode,
}
