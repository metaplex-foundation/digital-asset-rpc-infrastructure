#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("anchor")]
    Anchor(#[from] anchor_client::anchor_lang::error::Error),
    #[error("solana rpc")]
    Rpc(#[from] solana_client::client_error::ClientError),
    #[error("parse pubkey")]
    ParsePubkey(#[from] solana_sdk::pubkey::ParsePubkeyError),
    #[error("serialize tree response")]
    SerializeTreeResponse,
    #[error("sea orm")]
    Database(#[from] sea_orm::DbErr),
    #[error("try from pubkey")]
    TryFromPubkey,
    #[error("try from signature")]
    TryFromSignature,
    #[error("generic error: {0}")]
    Generic(String),
}
