use crate::DasApiError;
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
