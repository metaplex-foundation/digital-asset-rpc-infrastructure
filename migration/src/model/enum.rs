use enum_iterator::Sequence;
use sea_orm_migration::{prelude::*, sea_orm::EnumIter};

#[derive(Copy, Clone, Iden, EnumIter)]
pub enum Mutability {
    Immutable,
    Mutable,
    Unknown,
}

#[derive(Iden, Debug, PartialEq, Sequence)]
pub enum BubblegumInstruction {
    Unknown,
    MintV1,
    Redeem,
    CancelRedeem,
    Transfer,
    Delegate,
    DecompressV1,
    Compress,
    Burn,
    VerifyCreator,
    UnverifyCreator,
    VerifyCollection,
    UnverifyCollection,
    SetAndVerifyCollection,
    MintToCollectionV1,
    // Any new values cannot be added here, or else they will be added twice by the migrator (which will fail).
    // We need to use an alias instead.
    // UpdateMetadata,
}
