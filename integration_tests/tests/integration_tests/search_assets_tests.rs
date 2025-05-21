use std::str::FromStr;

use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;
use solana_sdk::signature::Signature;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_owner() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "2PfAwPb2hdgsf7xCKyU2kAWUGKnkxYZLfg5SMf4YP1h2",
        "Dt3XDSAdXAJbHqvuycgCTHykKCC7tntMFGMmSvfBbpTL",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress": "6Cr66AabRYymhZgYQSfTCo6FVpH18wXrMZswAbcErpyX",
        "page": 1,
        "limit": 2
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_creator() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "2PfAwPb2hdgsf7xCKyU2kAWUGKnkxYZLfg5SMf4YP1h2",
        "Dt3XDSAdXAJbHqvuycgCTHykKCC7tntMFGMmSvfBbpTL",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "creatorAddress": "47besT5AjYkf8bdtzxt7k8rZbKr612Z3cayEx2wHCtLn",
        "page": 1,
        "limit": 2
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_authority() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "2PfAwPb2hdgsf7xCKyU2kAWUGKnkxYZLfg5SMf4YP1h2",
        "Dt3XDSAdXAJbHqvuycgCTHykKCC7tntMFGMmSvfBbpTL",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "authorityAddress": "G2R7KKR9aycrMDzR2EK2Cs69f2NNBRK8AZUuaohQBg2r",
        "page": 1,
        "limit": 2
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_grouping() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "2PfAwPb2hdgsf7xCKyU2kAWUGKnkxYZLfg5SMf4YP1h2",
        "Dt3XDSAdXAJbHqvuycgCTHykKCC7tntMFGMmSvfBbpTL",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "grouping": ["collection", "1yPMtWU5aqcF72RdyRD5yipmcMRC8NGNK59NvYubLkZ"],
        "page": 1,
        "limit": 2
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

// By TokenType

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_regular_nft() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "42AYryUGNmJMe9ycBXZekkYvdTehgbtECHs7SLu5JJTB",
        "2w81QrLYTwSDkNwXgCqKAwrC1Tu6R9mh9BHcxys2Bup2",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
    "page": 1,
    "limit": 2,
    "tokenType": "regularNFT"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_non_fungible() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "AH6VcoSbCGGv8BHeN7K766VUWMcdFRTaXpLvGTLSdAmk",
        "8t77ShMViat27Sjphvi1FVPaGrhFcttPAkEnLCFp49Bo",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
    "page": 1,
    "limit": 2,
    "tokenType": "nonFungible"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_compressed() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_txns([
        "4nKDSvw2kGpccZWLEPnfdP7J1SEexQFRP3xWc9NBtQ1qQeGu3bu5WnAdpcLbjQ4iyX6BQ5QGF69wevE8ZeeY5poA",
        "4URwUGBjbsF7UBUYdSC546tnBy7nD67txsso8D9CR9kGLtbbYh9NkGw15tEp16LLasmJX5VQR4Seh8gDjTrtdpoC",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "53VVFtLzzi3nL2p1QF591PAB8rbcbsirYepwUphtHU9Q",
    "page": 1,
    "limit": 2,
    "tokenType": "compressedNFT"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_all_scenario_1() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "42AYryUGNmJMe9ycBXZekkYvdTehgbtECHs7SLu5JJTB",
        "8t77ShMViat27Sjphvi1FVPaGrhFcttPAkEnLCFp49Bo",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
    "page": 1,
    "limit": 2,
    "tokenType": "all"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_all_scenario_2() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_txns([
        "4nKDSvw2kGpccZWLEPnfdP7J1SEexQFRP3xWc9NBtQ1qQeGu3bu5WnAdpcLbjQ4iyX6BQ5QGF69wevE8ZeeY5poA",
        "4URwUGBjbsF7UBUYdSC546tnBy7nD67txsso8D9CR9kGLtbbYh9NkGw15tEp16LLasmJX5VQR4Seh8gDjTrtdpoC",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "53VVFtLzzi3nL2p1QF591PAB8rbcbsirYepwUphtHU9Q",
    "page": 1,
    "limit": 2,
    "tokenType": "all"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_fungible() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "7EYnhQoR9YM3N7UoaKRoA44Uy8JeaZV3qyouov87awMs",
        "7BajpcYgnxmWK91RhrfsdB3Tm83PcDwPvMC8ZinvtTY6",
        "6BRNfDfdq1nKyU1TQiCEQLWyPtD8EwUH9Kt2ahsbidUx",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
    "ownerAddress": "2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
    "page": 1,
    "limit": 1,
    "tokenType": "fungible"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_name() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seed: SeedEvent = seed_nft("2PfAwPb2hdgsf7xCKyU2kAWUGKnkxYZLfg5SMf4YP1h2");

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, vec![&seed]).await;

    let request = r#"        
    {
        "name": "Claynosaurz: Call of Saga #1121",
        "ownerAddress": "C2ch7QUCrYZRkhVVzTXojkdjhdJhaY77i4VQdoPS64HX",
        "page": 1,
        "limit": 2
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_by_compressed() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let transactions = vec![
        "25djDqCTka7wEnNMRVwsqSsVHqQMknPReTUCmvF4XGD9bUD494jZ1FsPaPjbAK45TxpdVuF2RwVCK9Jq7oxZAtMB",
        "3UrxyfoJKH2jvVkzZZuCMXxtyaFUtgjhoQmirwhyiFjZXA8oM3QCBixCSBj9b53t5scvsm3qpuq5Qm4cGbNuwQP7",
        "4fzBjTaXmrrJReLLSYPzn1fhPfuiU2EU1hGUddtHV1B49pvRewGyyzvMMpssi7K4Y5ZYj5xS9DrJuxqJDZRMZqY1",
    ];

    for tx in transactions {
        index_transaction(&setup, Signature::from_str(tx).unwrap()).await;
    }
    let request = r#"
    {
        "compressed":true
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request.clone()).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
