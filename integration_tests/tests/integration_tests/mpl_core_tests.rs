use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["x3hJtpU4AUsGejNvxzX9TKjcyNB1eYtDdDPWdeF6opr"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "id": "x3hJtpU4AUsGejNvxzX9TKjcyNB1eYtDdDPWdeF6opr"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_collection() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["DHciVfQxHHM7t2asQJRjjkKbjvZ4PuG3Y3uiULMQUjJQ"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "id": "DHciVfQxHHM7t2asQJRjjkKbjvZ4PuG3Y3uiULMQUjJQ"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_authority() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "9CSyGBw1DCVZfx621nb7UBM9SpVDsX1m9MaN6APCf1Ci",
        "4FcFVJVPRsYoMjt8ewDGV5nipoK63SNrJzjrBHyXvhcz",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "authorityAddress": "APrZTeVysBJqAznfLXS71NAzjr2fCVTSF1A66MeErzM7",
        "sortBy": {
            "sortBy": "updated",
            "sortDirection": "asc"
        },
        "page": 1,
        "limit": 50
    }
    "#;

    let request: api::GetAssetsByAuthority = serde_json::from_str(request).unwrap();
    let response = setup
        .das_api
        .get_assets_by_authority(request)
        .await
        .unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "JChzyyp1CnNz56tJLteQ5BsbngmWQ3JwcxLZrmuQA5b7",
        "kTMCCKLTaZsnSReer12HsciwScUwhHyZyd9D9BwQF8k",
        "EgzsppfYJmUet4ve8MnuHMyvSnj6R7LRmwsGEH5TuGhB",
        "J2kazVRuZ33Po4PVyZGxiDYUMQ1eZiT5Xa13usRYo264",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "groupKey": "collection",
        "groupValue": "JChzyyp1CnNz56tJLteQ5BsbngmWQ3JwcxLZrmuQA5b7",
        "sortBy": {
            "sortBy": "updated",
            "sortDirection": "asc"
        },
        "page": 1,
        "limit": 50
    }
    "#;

    let request: api::GetAssetsByGroup = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_owner() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "4FFhh184GNqh3LEK8UhMY7KBuCdNvvhU7C23ZKrKnofb",
        "9tsHoBrkSqBW5uMxKZyvxL6m9CCaz1a7sGEg8SuckUj",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "ownerAddress": "7uScVQiT4vArB88dHrZoeVKWbtsRJmNp9r5Gce5VQpXS",
        "sortBy": {
            "sortBy": "updated",
            "sortDirection": "asc"
        },
        "page": 1,
        "limit": 50
    }
    "#;

    let request: api::GetAssetsByOwner = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_owner(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset_with_edition() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["AejY8LGKAbQsrGZS1qgN4uFu99dJD3f8Js9Yrt7K3tCc"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "id": "AejY8LGKAbQsrGZS1qgN4uFu99dJD3f8Js9Yrt7K3tCc"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset_with_pubkey_in_rule_set() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["8H71x9Bhh9E9o3MZK4QnVC5MRFn1WZRf2Mc9w2wEbG5V"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "id": "8H71x9Bhh9E9o3MZK4QnVC5MRFn1WZRf2Mc9w2wEbG5V"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset_with_external_plugins() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["E8FwCS7D6pZa1L3oCeSdSesd3CC4rdZfAedubj1ru6aY"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "id": "E8FwCS7D6pZa1L3oCeSdSesd3CC4rdZfAedubj1ru6aY"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
