use digital_asset_types::dapi::{decode_change_cursor, encode_change_cursor};
use digital_asset_types::rpc::response::{AssetChangeItem, AssetChangeList};

#[test]
fn test_asset_change_list_serialization() {
    let list = AssetChangeList {
        current_slot: 42,
        items: vec![AssetChangeItem {
            id: "abc123".to_string(),
            slot_updated: 10,
            owner: Some("owner1".to_string()),
            delegate: Some("del1".to_string()),
            burnt: false,
            collection: Some("col1".to_string()),
            metadata_url: Some("https://example.com/meta.json".to_string()),
        }],
        after: Some("cursorXYZ".to_string()),
    };

    let json = serde_json::to_value(&list).unwrap();
    // Verify camelCase keys
    assert!(json.get("currentSlot").is_some());
    assert!(json.get("slotUpdated").is_none()); // top-level has no slotUpdated
    assert!(json.get("items").is_some());
    assert!(json.get("after").is_some());

    let item = &json["items"][0];
    assert!(item.get("slotUpdated").is_some());
    assert!(item.get("metadataUrl").is_some());
    assert_eq!(item["id"], "abc123");
    assert_eq!(item["slotUpdated"], 10);
    assert_eq!(item["owner"], "owner1");
    assert_eq!(item["delegate"], "del1");
    assert_eq!(item["burnt"], false);
    assert_eq!(item["collection"], "col1");
    assert_eq!(item["metadataUrl"], "https://example.com/meta.json");
}

#[test]
fn test_asset_change_list_none_fields_omitted() {
    let list = AssetChangeList {
        current_slot: 5,
        items: vec![AssetChangeItem {
            id: "id1".to_string(),
            slot_updated: 1,
            owner: None,
            delegate: None,
            burnt: true,
            collection: None,
            metadata_url: None,
        }],
        after: None,
    };

    let json = serde_json::to_value(&list).unwrap();
    // after should be omitted when None
    assert!(json.get("after").is_none());

    let item = &json["items"][0];
    // Optional None fields should be omitted
    assert!(item.get("owner").is_none());
    assert!(item.get("delegate").is_none());
    assert!(item.get("collection").is_none());
    assert!(item.get("metadataUrl").is_none());
    // Non-optional fields should still be present
    assert_eq!(item["burnt"], true);
    assert_eq!(item["id"], "id1");
    assert_eq!(item["slotUpdated"], 1);
}

#[test]
fn test_asset_change_list_empty() {
    let list = AssetChangeList {
        current_slot: 0,
        items: vec![],
        after: None,
    };

    let json = serde_json::to_value(&list).unwrap();
    assert_eq!(json["currentSlot"], 0);
    assert_eq!(json["items"], serde_json::json!([]));
    assert!(json.get("after").is_none());
}

#[test]
fn test_asset_change_list_round_trip() {
    let original = AssetChangeList {
        current_slot: 99,
        items: vec![
            AssetChangeItem {
                id: "abc".to_string(),
                slot_updated: 10,
                owner: Some("owner".to_string()),
                delegate: None,
                burnt: false,
                collection: None,
                metadata_url: None,
            },
            AssetChangeItem {
                id: "def".to_string(),
                slot_updated: 20,
                owner: None,
                delegate: None,
                burnt: true,
                collection: Some("col".to_string()),
                metadata_url: Some("https://example.com".to_string()),
            },
        ],
        after: Some("cursor".to_string()),
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: AssetChangeList = serde_json::from_str(&json_str).unwrap();
    assert_eq!(original, deserialized);
}

#[test]
fn test_cursor_round_trip() {
    let slot: i64 = 123_456_789;
    let asset_id: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

    let encoded = encode_change_cursor(slot, &asset_id);
    let (decoded_slot, decoded_id) = decode_change_cursor(&encoded).unwrap();

    assert_eq!(decoded_slot, slot);
    assert_eq!(decoded_id, asset_id);
}

#[test]
fn test_cursor_round_trip_zero_slot() {
    let slot: i64 = 0;
    let asset_id: Vec<u8> = vec![255];

    let encoded = encode_change_cursor(slot, &asset_id);
    let (decoded_slot, decoded_id) = decode_change_cursor(&encoded).unwrap();

    assert_eq!(decoded_slot, slot);
    assert_eq!(decoded_id, asset_id);
}

#[test]
fn test_cursor_round_trip_max_slot() {
    let slot: i64 = i64::MAX;
    let asset_id: Vec<u8> = vec![0; 32]; // typical Solana pubkey size

    let encoded = encode_change_cursor(slot, &asset_id);
    let (decoded_slot, decoded_id) = decode_change_cursor(&encoded).unwrap();

    assert_eq!(decoded_slot, slot);
    assert_eq!(decoded_id, asset_id);
}

#[test]
fn test_cursor_decode_invalid_too_short() {
    // 8 bytes = only slot, no asset id
    let short = bs58::encode(vec![0u8; 8]).into_string();
    assert!(decode_change_cursor(&short).is_none());

    // Even shorter
    let very_short = bs58::encode(vec![1, 2, 3]).into_string();
    assert!(decode_change_cursor(&very_short).is_none());
}

#[test]
fn test_cursor_decode_invalid_base58() {
    assert!(decode_change_cursor("not-valid-base58!!!").is_none());
}

#[test]
fn test_cursor_minimum_valid_size() {
    // 9 bytes = 8 byte slot + 1 byte id → minimum valid cursor
    let slot: i64 = 42;
    let asset_id: Vec<u8> = vec![7];

    let encoded = encode_change_cursor(slot, &asset_id);
    let (decoded_slot, decoded_id) = decode_change_cursor(&encoded).unwrap();

    assert_eq!(decoded_slot, slot);
    assert_eq!(decoded_id, asset_id);
}
