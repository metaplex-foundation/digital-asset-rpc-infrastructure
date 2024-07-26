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
                    .as_enum(BatchMintToVerify::BatchMintFailStatus)
                    .values([
                        FailedBatchMintState::ChecksumVerifyFailed,
                        FailedBatchMintState::BatchMintVerifyFailed,
                        FailedBatchMintState::DownloadFailed,
                        FailedBatchMintState::FileSerialization,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(BatchMintToVerify::BatchMintPersistingState)
                    .values([
                        PersistingBatchMintState::ReceivedTransaction,
                        PersistingBatchMintState::StartProcessing,
                        PersistingBatchMintState::FailedToPersist,
                        PersistingBatchMintState::SuccessfullyDownload,
                        PersistingBatchMintState::SuccessfullyValidate,
                        PersistingBatchMintState::StoredUpdate,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BatchMintToVerify::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BatchMintToVerify::FileHash)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BatchMintToVerify::Url).string().not_null())
                    .col(
                        ColumnDef::new(BatchMintToVerify::CreatedAtSlot)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BatchMintToVerify::Signature)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BatchMintToVerify::Staker)
                            .binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BatchMintToVerify::DownloadAttempts)
                            .unsigned()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BatchMintToVerify::BatchMintPersistingState)
                            .enumeration(
                                BatchMintToVerify::BatchMintPersistingState,
                                all::<PersistingBatchMintState>().collect::<Vec<_>>(),
                            )
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BatchMintToVerify::BatchMintFailStatus)
                            .enumeration(
                                BatchMintToVerify::BatchMintFailStatus,
                                all::<FailedBatchMintState>().collect::<Vec<_>>(),
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
                    .table(BatchMintToVerify::Table)
                    .col(BatchMintToVerify::CreatedAtSlot)
                    .col(BatchMintToVerify::BatchMintPersistingState)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BatchMint::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BatchMint::FileHash)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BatchMint::BatchMintBinaryBincode)
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
                "CREATE FUNCTION notify_new_batch_mint() RETURNS trigger LANGUAGE plpgsql AS $$
                    BEGIN
                      PERFORM pg_notify('new_batch_mint', NEW::text);
                      RETURN NEW;
                    END;
                    $$;
                    CREATE TRIGGER batch_mint_to_verify_trigger
                    AFTER INSERT ON batch_mint_to_verify
                    FOR EACH ROW EXECUTE FUNCTION notify_new_batch_mint();"
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
                "DROP TRIGGER IF EXISTS batch_mint_to_verify_trigger ON batch_mint_to_verify;
                DROP FUNCTION IF EXISTS notify_new_batch_mint;"
                    .to_string(),
            ))
            .await?;
        manager
            .drop_table(Table::drop().table(BatchMintToVerify::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BatchMint::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum BatchMintToVerify {
    Table,
    Url,
    FileHash,
    CreatedAtSlot,
    Signature,
    DownloadAttempts,
    BatchMintPersistingState,
    BatchMintFailStatus,
    Staker,
}

#[derive(Iden, Debug, PartialEq, Sequence)]
enum PersistingBatchMintState {
    ReceivedTransaction,
    FailedToPersist,
    StartProcessing,
    SuccessfullyDownload,
    SuccessfullyValidate,
    StoredUpdate,
}

#[derive(Iden, Debug, PartialEq, Sequence)]
enum FailedBatchMintState {
    DownloadFailed,
    ChecksumVerifyFailed,
    BatchMintVerifyFailed,
    FileSerialization,
}

#[derive(Iden)]
enum BatchMint {
    Table,
    FileHash,
    BatchMintBinaryBincode,
}
