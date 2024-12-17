use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

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
    "tokenType": "Nft"
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
    "tokenType": "NonFungible"
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
    "tokenType": "Compressed"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_search_asset_with_token_type_all() {
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
    "tokenType": "All"
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
    "page": 1,
    "limit": 1,
    "tokenType": "Fungible",
    "ownerType" : "token"
    }
    "#;

    let request: api::SearchAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
