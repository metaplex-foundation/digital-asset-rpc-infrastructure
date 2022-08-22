use crate::DasApiError;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub fn validate_pubkey(str_pubkey: String) -> Result<Pubkey, DasApiError> {
    Pubkey::from_str(&*str_pubkey).map_err(|_| DasApiError::PubkeyValidationError(str_pubkey))
}
