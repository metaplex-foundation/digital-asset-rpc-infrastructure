use sea_orm_migration::prelude::*;

#[derive(Copy, Clone, Iden)]
pub enum AssetCreators {
    Table,
    Id,
    AssetId,
    Creator,
    Position,
    Share,
    Verified,
    Seq,
}

#[derive(Copy, Clone, Iden)]
pub enum AssetAuthority {
    Table,
    Id,
    AssetId,
    Authority,
    SlotUpdated,
    Seq,
}

#[derive(Copy, Clone, Iden)]
pub enum AssetGrouping {
    Table,
    Id,
    AssetId,
    GroupKey,
    GroupValue,
    Seq,
    SlotUpdated,
    Verified,
    GroupInfoSeq,
}

#[derive(Copy, Clone, Iden)]
pub enum BackfillItems {
    Table,
    Id,
    Tree,
    Seq,
    Slot,
    ForceChk,
    Backfilled,
    Failed,
    Locked,
}

#[derive(Copy, Clone, Iden)]
pub enum Asset {
    Table,
    Id,
    AltId,
    SpecificationVersion,
    SpecificationAssetClass,
    Owner,
    OwnerType,
    Delegate,
    Frozen,
    Supply,
    SupplyMint,
    Compressed,
    Compressible,
    Seq,
    TreeId,
    Leaf,
    Nonce,
    RoyaltyTargetType,
    RoyaltyTarget,
    RoyaltyAmount,
    AssetData,
    CreatedAt,
    Burnt,
    SlotUpdated,
    SlotUpdatedMetadataAccount,
    SlotUpdatedMintAccount,
    SlotUpdatedTokenAccount,
    SlotUpdatedCnftTransaction,
    DataHash,
    CreatorHash,
    OwnerDelegateSeq,
    WasDecompressed,
    LeafSeq,
    BaseInfoSeq,
    MplCorePlugins,
    MplCoreUnknownPlugins,
    MplCoreCollectionNumMinted,
    MplCoreCollectionCurrentSize,
    MplCorePluginsJsonVersion,
    MplCoreExternalPlugins,
    MplCoreUnknownExternalPlugins,
}

#[derive(Copy, Clone, Iden)]
pub enum AssetData {
    Table,
    Id,
    ChainDataMutability,
    ChainData,
    MetadataUrl,
    MetadataMutability,
    Metadata,
    SlotUpdated,
    Reindex,
    RawName,
    RawSymbol,
    BaseInfoSeq,
}

#[derive(Copy, Clone, Iden)]
pub enum Tasks {
    Table,
    TaskType,
    Data,
    Status,
    CreatedAt,
    LockedUntil,
    LockedBy,
    MaxAttempts,
    Attempts,
    Duration,
    Errors,
}

#[derive(Copy, Clone, Iden)]
pub enum TokenAccounts {
    Table,
    Pubkey,
    Mint,
    Amount,
    Owner,
    Frozen,
    CloseAuthority,
    Delegate,
    DelegatedAmount,
    SlotUpdated,
    TokenProgram,
}

#[derive(Copy, Clone, Iden)]
pub enum Tokens {
    Table,
    Mint,
    Supply,
    Decimals,
    TokenProgram,
    MintAuthority,
    FreezeAuthority,
    CloseAuthority,
    SlotUpdated,
}

#[derive(Copy, Clone, Iden)]
pub enum ClAudits {
    Table,
    Id,
    Tree,
    NodeIdx,
    LeafIdx,
    Seq,
    Level,
    Hash,
    CreatedAt,
    Tx,
}

#[derive(Copy, Clone, Iden)]
pub enum ClAuditsV2 {
    Table,
    Id,
    Tree,
    LeafIdx,
    Seq,
    CreatedAt,
    Tx,
    Instruction,
}
