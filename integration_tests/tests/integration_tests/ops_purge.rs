#[cfg(test)]
use das_ops::purge::{
    start_cnft_purge, start_mint_purge, start_ta_purge, Args, CnftArgs, TOKEN_2022_PROGRAM_ID,
};
use digital_asset_types::dao::{
    cl_audits_v2, cl_items, sea_orm_active_enums::Instruction, token_accounts, tokens,
};
use itertools::Itertools;
use sea_orm::{
    DatabaseBackend, DbBackend, MockDatabase, MockExecResult, Statement, Transaction, Value, Values,
};
use solana_account_decoder::{UiAccount, UiAccountData};
use sqlx::types::Decimal;
use std::{
    collections::HashMap,
    sync::{Arc, Once},
};

use das_core::{DatabasePool, MockDatabasePool, Rpc};
use sea_orm::prelude::DateTime;
use serde_json::json;
use serial_test::serial;
use solana_client::{
    rpc_request::RpcRequest,
    rpc_response::{Response, RpcApiVersion, RpcResponseContext},
};
use solana_sdk::{
    message::MessageHeader,
    pubkey,
    pubkey::Pubkey,
    signature::Signature,
    transaction::{TransactionError, TransactionVersion},
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    EncodedTransaction, EncodedTransactionWithStatusMeta, UiCompiledInstruction, UiMessage,
    UiRawMessage, UiTransaction, UiTransactionStatusMeta,
};

static INIT: Once = Once::new();

const BGUM_PROGRAM_ID: Pubkey = pubkey!("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");

fn init_logger() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

fn system_program_account() -> UiAccount {
    UiAccount {
        lamports: 0,
        data: UiAccountData::LegacyBinary("".to_string()),
        owner: solana_sdk::system_program::id().to_string(),
        executable: false,
        rent_epoch: 0,
        space: Some(0),
    }
}

fn token_program_account() -> UiAccount {
    UiAccount {
        lamports: 100,
        data: UiAccountData::LegacyBinary("".to_string()),
        owner: TOKEN_2022_PROGRAM_ID.to_string(),
        executable: false,
        rent_epoch: 0,
        space: Some(165),
    }
}

fn token_account_model_with_pubkey(pubkey: &Pubkey) -> token_accounts::Model {
    token_accounts::Model {
        pubkey: pubkey.to_bytes().to_vec(),
        mint: Pubkey::new_unique().to_bytes().to_vec(),
        owner: Pubkey::new_unique().to_bytes().to_vec(),
        delegate: None,
        close_authority: None,
        amount: 0,
        frozen: false,
        delegated_amount: 0,
        slot_updated: 0,
        token_program: TOKEN_2022_PROGRAM_ID.to_bytes().to_vec(),
        extensions: None,
    }
}

fn mint_model_with_pubkey(pubkey: &Pubkey) -> tokens::Model {
    tokens::Model {
        mint: pubkey.to_bytes().to_vec(),
        decimals: 0,
        supply: Decimal::from(0),
        token_program: TOKEN_2022_PROGRAM_ID.to_bytes().to_vec(),
        mint_authority: None,
        freeze_authority: None,
        close_authority: None,
        slot_updated: 0,
        extensions: None,
        extension_data: None,
    }
}

fn encoded_confirmed_transaction_with_status_meta(
    err: Option<TransactionError>,
    signature: Signature,
) -> EncodedConfirmedTransactionWithStatusMeta {
    EncodedConfirmedTransactionWithStatusMeta {
        slot: 1,
        transaction: EncodedTransactionWithStatusMeta {
            version: Some(TransactionVersion::LEGACY),
            transaction: EncodedTransaction::Json(UiTransaction {
                signatures: vec![signature.to_string()],
                message: UiMessage::Raw(UiRawMessage {
                    header: MessageHeader {
                        num_required_signatures: 0,
                        num_readonly_signed_accounts: 0,
                        num_readonly_unsigned_accounts: 1,
                    },
                    account_keys: vec!["C6eBmAXKg6JhJWkajGa5YRGUfG4YKXwbxF5Ufv7PtExZ".to_string()],
                    recent_blockhash: "D37n3BSG71oUWcWjbZ37jZP7UfsxG2QMKeuALJ1PYvM6".to_string(),
                    instructions: vec![UiCompiledInstruction {
                        program_id_index: 2,
                        accounts: vec![0, 1],
                        data: "3Bxs49DitAvXtoDR".to_string(),
                        stack_height: None,
                    }],
                    address_table_lookups: None,
                }),
            }),
            meta: Some(UiTransactionStatusMeta {
                err,
                status: Ok(()),
                fee: 0,
                pre_balances: vec![499999999999999950, 50, 1],
                post_balances: vec![499999999999999950, 50, 1],
                inner_instructions: OptionSerializer::None,
                log_messages: OptionSerializer::None,
                pre_token_balances: OptionSerializer::None,
                post_token_balances: OptionSerializer::None,
                rewards: OptionSerializer::None,
                loaded_addresses: OptionSerializer::Skip,
                return_data: OptionSerializer::Skip,
                compute_units_consumed: OptionSerializer::Skip,
            }),
        },
        block_time: Some(1628633791),
    }
}

fn cl_audit_v2_model_with_tree() -> cl_audits_v2::Model {
    cl_audits_v2::Model {
        id: 0,
        tree: Pubkey::new_unique().to_bytes().to_vec(),
        seq: 0,
        leaf_idx: 0,
        created_at: DateTime::default(),
        tx: Signature::new_unique().as_ref().to_vec(),
        instruction: Instruction::MintV1,
    }
}

fn cl_items_model_with_tree(tree: Vec<u8>) -> cl_items::Model {
    cl_items::Model {
        id: 1,
        tree,
        seq: 0,
        leaf_idx: Some(0),
        node_idx: 1,
        level: 0,
        hash: vec![0],
    }
}

#[tokio::test]
#[serial]
async fn test_purging_token_accounts() {
    init_logger();

    let token_account_pubkeys: Vec<Pubkey> = (0..100).map(|_| Pubkey::new_unique()).collect();

    let ta_bytes_vec = token_account_pubkeys
        .iter()
        .map(|x| x.to_bytes())
        .collect::<Vec<[u8; 32]>>();

    println!("{:?}", ta_bytes_vec);

    let acc_missing_index: [usize; 5] = [1, 15, 22, 40, 89];

    let owner_mismatch_index: [usize; 5] = [20, 29, 33, 45, 71];

    let mock_query_res: Vec<token_accounts::Model> = token_account_pubkeys
        .iter()
        .map(token_account_model_with_pubkey)
        .collect();

    let mock_exec_res = MockExecResult {
        rows_affected: 10,
        last_insert_id: 0,
    };

    let mock_db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![mock_query_res])
        .append_exec_results(vec![mock_exec_res]);

    let db = Arc::new(MockDatabasePool::from(mock_db));
    let mut rpc_mock_responses = HashMap::new();

    let mut rpc_responses: Vec<Option<UiAccount>> = Vec::with_capacity(token_account_pubkeys.len());

    for i in 0..token_account_pubkeys.len() {
        if acc_missing_index.contains(&i) {
            rpc_responses.push(None);
        } else if owner_mismatch_index.contains(&i) {
            rpc_responses.push(Some(system_program_account()));
        } else {
            rpc_responses.push(Some(token_program_account()));
        }
    }

    let json_res = serde_json::to_value(rpc_responses).unwrap();

    rpc_mock_responses.insert(
        RpcRequest::GetMultipleAccounts,
        serde_json::json!(Response {
            context: RpcResponseContext {
                slot: 0,
                api_version: Some(RpcApiVersion::default()),
            },
            value: json_res
        }),
    );

    let rpc = Rpc::from_mocks(rpc_mock_responses, "succeeds".to_string());

    let res = start_ta_purge(
        Args {
            purge_worker_count: 10,
            mark_deletion_worker_count: 10,
            batch_size: 10,
            paginate_channel_size: 10,
        },
        Arc::clone(&db),
        rpc,
    )
    .await;
    assert!(res.is_ok());

    let ta_deleted_pubkeys: Vec<Pubkey> = acc_missing_index
        .iter()
        .chain(owner_mismatch_index.iter())
        .sorted()
        .map(|&i| token_account_pubkeys[i])
        .collect();

    let db_txs = db.connection().into_transaction_log();

    let ta_select_query =
        r#"SELECT "token_accounts"."pubkey", "token_accounts"."mint", "token_accounts"."amount",
 "token_accounts"."owner", "token_accounts"."frozen", "token_accounts"."close_authority",
 "token_accounts"."delegate", "token_accounts"."delegated_amount", "token_accounts"."slot_updated",
 "token_accounts"."token_program", "token_accounts"."extensions", "token_accounts"."pubkey"
 FROM "token_accounts" LIMIT $1 OFFSET $2"#
            .replace("\n", "");

    let expected_db_txs = vec![
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            ta_select_query.as_str(),
            vec![10u64.into(), 0u64.into()],
        ),
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            ta_select_query.as_str(),
            vec![10u64.into(), 10u64.into()],
        ),
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            r#"DELETE FROM "token_accounts" WHERE "token_accounts"."pubkey" IN ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
            ta_deleted_pubkeys
                .iter()
                .map(|x| Value::from(x.to_bytes().to_vec()))
                .collect::<Vec<_>>(),
        ),
    ];

    assert_eq!(expected_db_txs, db_txs);

    assert!(res.is_ok());
}

#[tokio::test]
#[serial]
async fn test_purging_mints() {
    init_logger();

    let mint_pubkeys: Vec<Pubkey> = (0..100).map(|_| Pubkey::new_unique()).collect();

    let acc_missing_index: [usize; 5] = [1, 15, 22, 40, 89];

    let owner_mismatch_index: [usize; 5] = [20, 29, 33, 45, 71];

    let mock_query_res_1: Vec<tokens::Model> =
        mint_pubkeys.iter().map(mint_model_with_pubkey).collect();

    let mock_exec_res_1 = MockExecResult {
        rows_affected: (acc_missing_index.len() + owner_mismatch_index.len()) as u64,
        last_insert_id: 0,
    };

    let mock_db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![mock_query_res_1])
        .append_exec_results(vec![mock_exec_res_1.clone()])
        .append_exec_results(vec![mock_exec_res_1]);

    let db = Arc::new(MockDatabasePool::from(mock_db));

    let mut rpc_mock_responses = HashMap::new();

    let mut rpc_responses: Vec<Option<UiAccount>> = Vec::with_capacity(mint_pubkeys.len());

    for i in 0..mint_pubkeys.len() {
        if acc_missing_index.contains(&i) {
            rpc_responses.push(None);
        } else if owner_mismatch_index.contains(&i) {
            rpc_responses.push(Some(system_program_account()));
        } else {
            rpc_responses.push(Some(token_program_account()));
        }
    }

    let json_res = serde_json::to_value(rpc_responses).unwrap();

    rpc_mock_responses.insert(
        RpcRequest::GetMultipleAccounts,
        serde_json::json!(Response {
            context: RpcResponseContext {
                slot: 0,
                api_version: Some(RpcApiVersion::default()),
            },
            value: json_res
        }),
    );

    let rpc = Rpc::from_mocks(rpc_mock_responses, "succeeds".to_string());

    let res = start_mint_purge(
        Args {
            purge_worker_count: 10,
            mark_deletion_worker_count: 10,
            batch_size: 10,
            paginate_channel_size: 10,
        },
        Arc::clone(&db),
        rpc,
    )
    .await;

    let db_txs = db.connection().into_transaction_log();

    let mint_deleted_pubkeys: Vec<Pubkey> = acc_missing_index
        .iter()
        .chain(owner_mismatch_index.iter())
        .sorted()
        .map(|&i| mint_pubkeys[i])
        .collect();

    let tokens_select_query = r#"SELECT "tokens"."mint", "tokens"."supply", "tokens"."decimals", "tokens"."token_program",
 "tokens"."mint_authority", "tokens"."freeze_authority", "tokens"."close_authority", "tokens"."extension_data",
 "tokens"."slot_updated", "tokens"."extensions", "tokens"."mint"
 FROM "tokens" LIMIT $1 OFFSET $2"#.replace("\n", "");

    let expected_db_txs = vec![
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            tokens_select_query.as_str(),
            vec![10u64.into(), 0u64.into()],
        ),
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            tokens_select_query.as_str(),
            vec![10u64.into(), 10u64.into()],
        ),
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            r#"DELETE FROM "tokens" WHERE "tokens"."mint" IN ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
            mint_deleted_pubkeys
                .iter()
                .map(|x| Value::from(x.to_bytes().to_vec()))
                .collect::<Vec<_>>(),
        ),
        Transaction::from_sql_and_values(
            DbBackend::Postgres,
            r#"UPDATE "asset" SET "burnt" = $1 WHERE "asset"."burnt" = $2 AND "asset"."id" IN ($3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
            vec![Value::Bool(Some(true)), Value::Bool(Some(false))]
                .into_iter()
                .chain(
                    mint_deleted_pubkeys
                        .iter()
                        .map(|x| Value::from(x.to_bytes().to_vec())),
                )
                .collect::<Vec<_>>(),
        ),
    ];

    assert_eq!(expected_db_txs, db_txs);

    assert!(res.is_ok());
}

#[tokio::test]
#[serial]
async fn test_skipping_purge_cnft_on_successful_transaction() {
    init_logger();

    // Set DB
    let mock_cl_audit_v2_model = cl_audit_v2_model_with_tree();

    let mock_db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![mock_cl_audit_v2_model.clone()]]);

    let db = Arc::new(MockDatabasePool::from(mock_db));

    // Set RPC
    let mut rpc_mock_responses = HashMap::new();
    let tx_sig = Signature::try_from(mock_cl_audit_v2_model.tx.as_slice()).unwrap();
    let rpc_response = encoded_confirmed_transaction_with_status_meta(None, tx_sig);

    rpc_mock_responses.insert(RpcRequest::GetTransaction, json!(rpc_response));

    let rpc = Rpc::from_mocks(rpc_mock_responses, "succeeds".to_string());

    let res = start_cnft_purge(
        CnftArgs {
            only_trees: None,
            purge_args: Args {
                purge_worker_count: 10,
                mark_deletion_worker_count: 10,
                batch_size: 10,
                paginate_channel_size: 10,
            },
        },
        Arc::clone(&db),
        rpc,
    )
    .await;

    let select_batch_query = Transaction::from_sql_and_values(
        DbBackend::Postgres,
        r#"SELECT "cl_audits_v2"."id", "cl_audits_v2"."tx", "cl_audits_v2"."leaf_idx", "cl_audits_v2"."tree", "cl_audits_v2"."seq" FROM "cl_audits_v2" WHERE "cl_audits_v2"."id" > $1 ORDER BY "cl_audits_v2"."id" ASC LIMIT $2"#,
        vec![0i64.into(), 10u64.into()],
    );

    // We expect the query 2 times the 1st one is actually going to query all db records and the 2nd one
    //  is going to return an empty response that will make the query loop break
    let expected_transactions: Vec<Transaction> = vec![select_batch_query; 2];

    assert_eq!(
        expected_transactions,
        db.connection().into_transaction_log()
    );
    assert!(res.is_ok());
}

#[tokio::test]
#[serial]
async fn test_purging_cnft_with_failed_transaction() {
    init_logger();

    // Set DB
    let mock_cl_audit_v2_model = cl_audit_v2_model_with_tree();
    let tree_bytes = mock_cl_audit_v2_model.tree.clone();
    let mock_cl_item_model = cl_items_model_with_tree(tree_bytes.clone());

    let (asset, _) = Pubkey::find_program_address(
        &[
            b"asset",
            &tree_bytes,
            &mock_cl_audit_v2_model.leaf_idx.to_le_bytes(),
        ],
        &BGUM_PROGRAM_ID,
    );
    let asset_bytes = asset.to_bytes().to_vec();

    let mock_db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![mock_cl_audit_v2_model.clone()], vec![]])
        .append_query_results(vec![vec![mock_cl_item_model.clone()]])
        .append_exec_results(vec![
            MockExecResult {
                rows_affected: 1,
                last_insert_id: 0,
            };
            7
        ]);

    let db = Arc::new(MockDatabasePool::from(mock_db));

    // Set RPC
    let mut rpc_mock_responses = HashMap::new();
    let rpc_response = encoded_confirmed_transaction_with_status_meta(
        Some(TransactionError::AccountInUse),
        Signature::new_unique(),
    );

    rpc_mock_responses.insert(RpcRequest::GetTransaction, json!(rpc_response));

    let rpc = Rpc::from_mocks(rpc_mock_responses, "succeeds".to_string());

    let res = start_cnft_purge(
        CnftArgs {
            only_trees: None,
            purge_args: Args {
                purge_worker_count: 1,
                mark_deletion_worker_count: 1,
                batch_size: 10,
                paginate_channel_size: 10,
            },
        },
        Arc::clone(&db),
        rpc,
    )
    .await;

    assert!(res.is_ok());

    let select_batch_query = Transaction::from_sql_and_values(
        DbBackend::Postgres,
        r#"SELECT "cl_audits_v2"."id", "cl_audits_v2"."tx", "cl_audits_v2"."leaf_idx", "cl_audits_v2"."tree", "cl_audits_v2"."seq" FROM "cl_audits_v2" WHERE "cl_audits_v2"."id" > $1 ORDER BY "cl_audits_v2"."id" ASC LIMIT $2"#,
        vec![0i64.into(), 10u64.into()],
    );

    let select_cl_items = Transaction::from_sql_and_values(
        DbBackend::Postgres,
        r#"SELECT "cl_items"."id", "cl_items"."tree", "cl_items"."node_idx", "cl_items"."leaf_idx", "cl_items"."seq", "cl_items"."level", "cl_items"."hash" FROM "cl_items" WHERE "cl_items"."tree" = $1 AND "cl_items"."leaf_idx" = $2 LIMIT $3"#,
        vec![
            Value::Bytes(Some(Box::new(tree_bytes.clone()))),
            Value::BigInt(Some(0)),
            Value::BigUnsigned(Some(1)),
        ],
    );

    let asset_values = Some(Values(vec![Value::Bytes(Some(Box::new(asset_bytes)))]));

    let delete_tx = Transaction::many(vec![
        Statement {
            sql: "BEGIN".to_string(),
            values: None,
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "asset_data" WHERE "asset_data"."id" = $1"#.to_string(),
            values: asset_values.clone(),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "asset" WHERE "asset"."id" = $1"#.to_string(),
            values: asset_values.clone(),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "asset_creators" WHERE "asset_creators"."asset_id" = $1"#
                .to_string(),
            values: asset_values.clone(),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "asset_authority" WHERE "asset_authority"."asset_id" = $1"#
                .to_string(),
            values: asset_values.clone(),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "asset_grouping" WHERE "asset_grouping"."asset_id" = $1"#
                .to_string(),
            values: asset_values.clone(),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "cl_items" WHERE "cl_items"."tree" = $1 AND "cl_items"."node_idx" IN ($2) AND "cl_items"."seq" <= $3"#.to_string(),
            values: Some(Values(vec![
                Value::Bytes(Some(Box::new(tree_bytes.clone()))),
                Value::BigInt(Some(1)),
                Value::BigInt(Some(0)),
            ])),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: r#"DELETE FROM "cl_audits_v2" WHERE "cl_audits_v2"."leaf_idx" = $1 AND "cl_audits_v2"."tree" = $2"#.to_string(),
            values: Some(Values(vec![
                Value::BigInt(Some(0)),
                Value::Bytes(Some(Box::new(tree_bytes))),
            ])),
            db_backend: DbBackend::Postgres,
        },
        Statement {
            sql: "COMMIT".to_string(),
            values: None,
            db_backend: DbBackend::Postgres,
        },
    ]);

    let expected_transactions: Vec<Transaction> = vec![
        select_batch_query.clone(),
        select_batch_query,
        select_cl_items,
        delete_tx,
    ];

    assert_eq!(
        expected_transactions,
        db.connection().into_transaction_log()
    );
}
