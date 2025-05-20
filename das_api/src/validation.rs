use crate::error::DasApiError;
use digital_asset_types::dao::scopes::token::{SPL_TOKEN, SPL_TOKEN_2022};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub fn validate_pubkey(str_pubkey: String) -> Result<Pubkey, DasApiError> {
    Pubkey::from_str(&str_pubkey).map_err(|_| DasApiError::PubkeyValidationError(str_pubkey))
}

pub fn validate_search_with_name(
    name: &Option<String>,
    owner: &Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, DasApiError> {
    let opt_name = if let Some(n) = name {
        if owner.is_none() {
            return Err(DasApiError::ValidationError(
                "Owner address must be provided in order to search assets by name".to_owned(),
            ));
        }
        Some(n.clone().into_bytes())
    } else {
        None
    };
    Ok(opt_name)
}

pub fn validate_opt_pubkey(pubkey: &Option<String>) -> Result<Option<Vec<u8>>, DasApiError> {
    let opt_bytes = if let Some(pubkey) = pubkey {
        let pubkey = Pubkey::from_str(pubkey)
            .map_err(|_| DasApiError::ValidationError(format!("Invalid pubkey {}", pubkey)))?;
        Some(pubkey.to_bytes().to_vec())
    } else {
        None
    };
    Ok(opt_bytes)
}

pub fn validate_opt_token_program(
    token_program: &Option<String>,
) -> Result<Option<Vec<u8>>, DasApiError> {
    let validated_pubkey = validate_opt_pubkey(token_program)?;

    let opt_bytes = if let Some(token_program) = validated_pubkey {
        let token_program_pubkey = bs58::encode(token_program.clone()).into_string();
        if token_program_pubkey.eq(SPL_TOKEN) || token_program_pubkey.eq(SPL_TOKEN_2022) {
            Some(token_program)
        } else {
            return Err(DasApiError::InvalidProgramId(token_program_pubkey));
        }
    } else {
        None
    };

    Ok(opt_bytes)
}
