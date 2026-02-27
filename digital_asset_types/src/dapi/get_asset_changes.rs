use sea_orm::{DatabaseConnection, DbErr};

use crate::{
    dao::scopes::asset,
    rpc::response::{AssetChangeItem, AssetChangeList},
};

pub fn encode_change_cursor(slot: i64, id: &[u8]) -> String {
    let mut cursor_bytes = Vec::with_capacity(8 + id.len());
    cursor_bytes.extend_from_slice(&slot.to_be_bytes());
    cursor_bytes.extend_from_slice(id);
    bs58::encode(cursor_bytes).into_string()
}

/// Decodes a change cursor into (slot, asset_id).
/// Returns `None` if the cursor is invalid (bad base58 or too short).
pub fn decode_change_cursor(cursor: &str) -> Option<(i64, Vec<u8>)> {
    let bytes = bs58::decode(cursor).into_vec().ok()?;
    if bytes.len() < 9 {
        return None;
    }
    let slot_bytes: [u8; 8] = bytes[..8].try_into().ok()?;
    let slot = i64::from_be_bytes(slot_bytes);
    let id = bytes[8..].to_vec();
    Some((slot, id))
}

pub async fn get_asset_changes(
    db: &DatabaseConnection,
    after_slot: Option<i64>,
    cursor_slot: Option<i64>,
    cursor_id: Option<Vec<u8>>,
    limit: u64,
) -> Result<AssetChangeList, DbErr> {
    let (rows, current_slot) =
        asset::get_asset_changes(db, after_slot, cursor_slot, cursor_id, limit).await?;

    let after = rows.last().and_then(|last| {
        last.slot_updated
            .map(|slot| encode_change_cursor(slot, &last.id))
    });

    let items = rows
        .into_iter()
        .map(|row| AssetChangeItem {
            id: bs58::encode(&row.id).into_string(),
            slot_updated: row.slot_updated.unwrap_or(0),
            owner: row.owner.map(|o| bs58::encode(o).into_string()),
            delegate: row.delegate.map(|d| bs58::encode(d).into_string()),
            burnt: row.burnt,
            collection: row.collection,
            metadata_url: row.metadata_url,
        })
        .collect();

    Ok(AssetChangeList {
        current_slot,
        items,
        after,
    })
}
