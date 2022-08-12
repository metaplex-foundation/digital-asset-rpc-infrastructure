use {crate::error::IngesterError, anchor_client::anchor_lang, base64};

pub fn handle_event<T: anchor_lang::Event + anchor_lang::AnchorDeserialize>(
    data: String,
) -> Result<T, IngesterError> {
    let borsh_bytes = match base64::decode(&data) {
        Ok(borsh_bytes) => borsh_bytes,
        _ => {
            return Err(IngesterError::ChangeLogEventMalformed);
        }
    };

    let mut slice: &[u8] = &borsh_bytes[..];
    let disc: [u8; 8] = {
        let mut disc = [0; 8];
        disc.copy_from_slice(&borsh_bytes[..8]);
        slice = &slice[8..];
        disc
    };
    if disc != T::discriminator() {
        return Err(IngesterError::ChangeLogEventMalformed);
    }

    let e: T = anchor_lang::AnchorDeserialize::deserialize(&mut slice)
        .map_err(|_| IngesterError::ChangeLogEventMalformed)?;
    Ok(e)
}
