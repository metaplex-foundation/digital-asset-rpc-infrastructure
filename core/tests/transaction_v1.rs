// Regression test for Solana transaction v1 (SIMD-0296 / SIMD-0385).
// The fixture is a real devnet v1 transaction, slot 495584494.
use das_core::{serialize_encoded_transaction_with_status, PLERKLE_TRANSACTION_VERSION_V1};
use flatbuffers::FlatBufferBuilder;
use plerkle_serialization::root_as_transaction_info;
use solana_sdk::{message::VersionedMessage, transaction::VersionedTransaction};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;

fn fixture() -> EncodedConfirmedTransactionWithStatusMeta {
    let raw = include_str!("fixtures/transaction_v1.json");
    serde_json::from_str(raw).expect("fixture parses")
}

#[test]
fn decodes_a_real_v1_transaction() {
    let tx = fixture();

    let decoded: VersionedTransaction = tx
        .transaction
        .transaction
        .decode()
        .expect("v1 transaction decodes");

    assert!(
        matches!(decoded.message, VersionedMessage::V1(_)),
        "expected a v1 message"
    );

    // v1 drops address lookup tables, so every key is static.
    assert!(decoded.message.address_table_lookups().is_none());
    assert_eq!(decoded.message.static_account_keys().len(), 2);
    assert_eq!(decoded.message.instructions().len(), 1);

    // SIMD-0296: this transaction is larger than the legacy 1232-byte cap.
    assert!(
        decoded.message.serialize().len() > 1232,
        "fixture should exceed the legacy transaction size limit"
    );
}

#[test]
fn serializes_a_v1_transaction_for_the_plerkle_stream() {
    let tx = fixture();
    let slot = tx.slot;

    let builder = serialize_encoded_transaction_with_status(FlatBufferBuilder::new(), tx)
        .expect("v1 transaction serializes");

    let info = root_as_transaction_info(builder.finished_data()).expect("flatbuffer verifies");

    assert_eq!(info.version(), PLERKLE_TRANSACTION_VERSION_V1);
    assert_eq!(info.slot(), slot);
    assert_eq!(info.account_keys().expect("account keys").len(), 2);
    assert_eq!(info.outer_instructions().expect("instructions").len(), 1);
}
