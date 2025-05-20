use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_accounts_by_mint() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["jKLTJu7nE1zLmC2J2xjVVBm4G7vJcKGCGQX36Jrsba2"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "mintAddress":"wKocBVvHQoVaiwWoCs9JYSVye4YZRrv5Cucf7fDqnz1"
    }
    "#;

    let request: api::GetTokenAccounts = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_token_accounts(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_accounts_by_owner() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "jKLTJu7nE1zLmC2J2xjVVBm4G7vJcKGCGQX36Jrsba2",
        "3Pv9H5UzU8T9BwgutXrcn2wLohS1JUZuk3x8paiRyzui",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress":"CeviT1DTQLuicEB7yLeFkkAGmam5GnJssbGb7CML4Tgx"
    }
    "#;

    let request: api::GetTokenAccounts = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_token_accounts(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_largest_accounts() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "WzWUoCmtVv7eqAbU3BfKPU3fhLP6CXR8NCJH78UK9VS",
        "81BadRGfaHFpAmuXpJ65k8tYtUWsZ54EFSmsVo1rbDTV",
        "FzbcyEZ9m8xjtergWgWDq7mfPoHEbboBF791B6cTpzbq",
        "GXWqPpjQpdz7KZw9p7f5PX2eGxHAhvpNXiviFkAB8zXg",
        "GENey8es3EgGiNTM8H8gzA3vf98haQF8LHiYFyErjgrv",
        "Bgq7trRgVMeq33yt235zM2onQ4bRDBsY5EWiTetF4qw6",
        "ESDXSfcVfDhPFDNqH7qMXVcC5fEiVpcQcqxuNjKYjx9m",
        "GiMk2kEib3P3FqUwkiwTky4JwxChrwgvVo7HGTJT4Z7z",
        "7jaiZR5Sk8hdYN9MxTpczTcwbWpb5WEoxSANuUwveuat",
        "8SheGtsopRUDzdiD6v6BR9a6bqZ9QwywYQY99Fp5meNf",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;

    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let slots: Vec<SeedEvent> = seed_slots([100, 99, 98]);
    index_seed_events(&setup, slots.iter().collect_vec()).await;

    let request = r#"
    [
      "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      {
        "commitment": "confirmed"
      }
    ]
    "#;

    let request: api::GetTokenLargestAccounts = serde_json::from_str(request).unwrap();
    let response = setup
        .das_api
        .get_token_largest_accounts(request)
        .await
        .unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_supply() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    [
      "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      {
        "commitment": "confirmed"
      }
    ]
    "#;

    let request: api::GetTokenSupply = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_token_supply(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_accounts_by_owner_rpc() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "jKLTJu7nE1zLmC2J2xjVVBm4G7vJcKGCGQX36Jrsba2",
        "3Pv9H5UzU8T9BwgutXrcn2wLohS1JUZuk3x8paiRyzui",
        "F3D8Priw3BRecH36BuMubQHrTUn1QxmupLHEmmbZ4LXW",
        "wKocBVvHQoVaiwWoCs9JYSVye4YZRrv5Cucf7fDqnz1",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    [
      "CeviT1DTQLuicEB7yLeFkkAGmam5GnJssbGb7CML4Tgx",
      {
        "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
      },
      {
        "commitment": "confirmed",
        "encoding": "jsonParsed"
      }
    ]
    "#;

    let request: api::GetTokenAccountsByOwner = serde_json::from_str(request).unwrap();
    let response = setup
        .das_api
        .get_token_accounts_by_owner(request)
        .await
        .unwrap();

    insta::assert_json_snapshot!(name, response);
}
