use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, pubkeys};

use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
};

use super::ProgramParseResult;

pubkeys!(
    inscription_program_id,
    "inscokhJarcjaEs59QbQ7hYjrKz25LEPRfCbP8EmdUp"
);

pub struct TokenInscriptionParser;

#[derive(Debug, Serialize, Deserialize)]
pub struct InscriptionData {
    pub authority: String,
    pub root: String,
    pub content: String,
    pub encoding: String,
    pub inscription_data: String,
    pub order: u64,
    pub size: u32,
    pub validation_hash: Option<String>,
}

impl InscriptionData {
    pub const BASE_SIZE: usize = 121;
    pub const INSCRIPTION_ACC_DATA_DISC: [u8; 8] = [232, 120, 205, 47, 153, 239, 229, 224];

    pub fn try_unpack_data(data: &[u8]) -> Result<Self, BlockbusterError> {
        let acc_disc = &data[0..8];

        if acc_disc != Self::INSCRIPTION_ACC_DATA_DISC {
            return Err(BlockbusterError::InvalidAccountType);
        }

        if data.len() < Self::BASE_SIZE {
            return Err(BlockbusterError::CustomDeserializationError(
                "Inscription Data is too short".to_string(),
            ));
        }

        let authority = Pubkey::try_from(&data[8..40]).unwrap();
        let mint = Pubkey::try_from(&data[40..72]).unwrap();
        let inscription_data = Pubkey::try_from(&data[72..104]).unwrap();
        let order = u64::from_le_bytes(data[104..112].try_into().unwrap());
        let size = u32::from_le_bytes(data[112..116].try_into().unwrap());
        let content_type_len = u32::from_le_bytes(data[116..120].try_into().unwrap()) as usize;
        let content = String::from_utf8(data[120..120 + content_type_len].to_vec()).unwrap();
        let encoding_len = u32::from_le_bytes(
            data[120 + content_type_len..124 + content_type_len]
                .try_into()
                .unwrap(),
        ) as usize;

        let encoding = String::from_utf8(
            data[124 + content_type_len..124 + content_type_len + encoding_len].to_vec(),
        )
        .unwrap();

        let validation_exists = u8::from_le_bytes(
            data[124 + content_type_len + encoding_len..124 + content_type_len + encoding_len + 1]
                .try_into()
                .unwrap(),
        );

        let validation_hash = if validation_exists == 1 {
            let validation_hash_len = u32::from_le_bytes(
                data[124 + content_type_len + encoding_len + 1
                    ..128 + content_type_len + encoding_len + 1]
                    .try_into()
                    .unwrap(),
            ) as usize;
            Some(
                String::from_utf8(
                    data[128 + content_type_len + encoding_len + 1
                        ..128 + content_type_len + encoding_len + 1 + validation_hash_len]
                        .to_vec(),
                )
                .unwrap(),
            )
        } else {
            None
        };
        Ok(InscriptionData {
            authority: authority.to_string(),
            root: mint.to_string(),
            content,
            encoding,
            inscription_data: inscription_data.to_string(),
            order,
            size,
            validation_hash,
        })
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
        inscription_program_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &inscription_program_id()
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
        let data = InscriptionData::try_unpack_data(account_data)?;
        Ok(Box::new(TokenInscriptionAccount { data }))
    }
}
