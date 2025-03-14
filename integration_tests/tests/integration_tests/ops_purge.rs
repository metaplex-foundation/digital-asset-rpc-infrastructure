#[cfg(test)]
use das_ops::purge::{start_mint_purge, start_ta_purge, Args, TOKEN_2022_PROGRAM_ID};
use digital_asset_types::dao::{token_accounts, tokens};
use itertools::Itertools;
use sea_orm::{DatabaseBackend, DbBackend, MockDatabase, MockExecResult, Transaction, Value};
use solana_account_decoder::{UiAccount, UiAccountData};
use sqlx::types::Decimal;
use std::{
    collections::HashMap,
    sync::{Arc, Once},
};

use das_core::{DatabasePool, MockDatabasePool, Rpc};

use serial_test::serial;
use solana_client::{
    rpc_request::RpcRequest,
    rpc_response::{Response, RpcApiVersion, RpcResponseContext},
};
use solana_sdk::pubkey::Pubkey;

static INIT: Once = Once::new();

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
