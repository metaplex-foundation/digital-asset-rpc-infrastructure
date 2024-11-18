use borsh::BorshDeserialize;
use libreplex_inscriptions::{EncodingType, Inscription, MediaType};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, pubkeys};

use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
};

use super::ProgramParseResult;

pubkeys!(
    inscription_progran_id,
    "inscokhJarcjaEs59QbQ7hYjrKz25LEPRfCbP8EmdUp"
);

pub struct TokenInscriptionParser;

#[derive(Debug, Serialize, Deserialize)]
pub enum InscriptionMediaType {
    None,
    Audio { subtype: String },
    Application { subtype: String },
    Image { subtype: String },
    Video { subtype: String },
    Text { subtype: String },
    Custom { media_type: String },
    Erc721,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum InscriptionEncodingType {
    None,
    Base64,
}

impl From<EncodingType> for InscriptionEncodingType {
    fn from(encoding_type: EncodingType) -> Self {
        match encoding_type {
            EncodingType::None => InscriptionEncodingType::None,
            EncodingType::Base64 => InscriptionEncodingType::Base64,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InscriptionData {
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub media_type: InscriptionMediaType,
    pub encoding: InscriptionEncodingType,
    pub inscription_data: Pubkey,
    pub order: u64,
    pub size: u32,
    pub validation_hash: Option<String>,
}

impl From<MediaType> for InscriptionMediaType {
    fn from(media_type: MediaType) -> Self {
        match media_type {
            MediaType::None => InscriptionMediaType::None,
            MediaType::Audio { subtype } => InscriptionMediaType::Audio { subtype },
            MediaType::Application { subtype } => InscriptionMediaType::Application { subtype },
            MediaType::Image { subtype } => InscriptionMediaType::Image { subtype },
            MediaType::Video { subtype } => InscriptionMediaType::Video { subtype },
            MediaType::Text { subtype } => InscriptionMediaType::Text { subtype },
            MediaType::Custom { media_type } => InscriptionMediaType::Custom { media_type },
            MediaType::Erc721 => InscriptionMediaType::Erc721,
        }
    }
}

pub struct TokenInscriptionAccount {
    pub data: InscriptionData,
}

impl ParseResult for TokenInscriptionAccount {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::TokenInscriptionAccount(self)
    }
}

impl ProgramParser for TokenInscriptionParser {
    fn key(&self) -> Pubkey {
        inscription_progran_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &inscription_progran_id()
    }

    fn handles_account_updates(&self) -> bool {
        true
    }

    fn handles_instructions(&self) -> bool {
        false
    }

    fn handle_account(
        &self,
        account_data: &[u8],
    ) -> Result<Box<(dyn ParseResult + 'static)>, BlockbusterError> {
        let inscription = Inscription::try_from_slice(account_data).map_err(|_| {
            BlockbusterError::CustomDeserializationError("Inscription Unpack Failed".to_string())
        })?;

        let data = InscriptionData {
            authority: inscription.authority,
            mint: inscription.root,
            media_type: inscription.media_type.into(),
            encoding: inscription.encoding_type.into(),
            inscription_data: inscription.inscription_data,
            order: inscription.order,
            size: inscription.size,
            validation_hash: inscription.validation_hash,
        };

        Ok(Box::new(TokenInscriptionAccount { data }))
    }
}
