use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]

async fn test_get_asset_with_show_collection_metadata_option() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
        "3crBqZZsHhoLphM55MG4KRW6SbNzFEBFnehw7PW7ZRKt",
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
        "8KyuwGzav7jTW9YaBGj2Qtp2q24zPUR3rD5caojXaby4",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;

    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    index_metadata_jsons(
        &setup,
        &[
            "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
            "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
        ],
        serde_json::from_str(SAMPLE_METADATA_JSON).unwrap(),
    )
    .await;

    let request = r#"
    {
    "id": "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
    "displayOptions" : {
        "showCollectionMetadata": true
        }
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();

    let response = setup.das_api.get_asset(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_by_group_with_show_collection_metadata_option() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
        "3crBqZZsHhoLphM55MG4KRW6SbNzFEBFnehw7PW7ZRKt",
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
        "8KyuwGzav7jTW9YaBGj2Qtp2q24zPUR3rD5caojXaby4",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;

    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    index_metadata_jsons(
        &setup,
        &[
            "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
            "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
        ],
        serde_json::from_str(SAMPLE_METADATA_JSON).unwrap(),
    )
    .await;

    let request = r#"
    {
        "groupKey": "collection",
        "groupValue": "J1S9H3QjnRtBbbuD4HjPV6RpRhwuk4zKbxsnCHuTgh9w",
        "displayOptions": {
            "showCollectionMetadata": true
        },
        "limit": 50,
        "page": 1
    }
    "#;

    let request: api::GetAssetsByGroup = serde_json::from_str(request).unwrap();

    let response = setup.das_api.get_assets_by_group(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_by_owner_with_show_fungible() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "Ff3Ci7EnM6KUdNDtzZvCjC5APgz1ajYW2Pt2BfT6Wftq",
        "7mX5Gzny3r8WBRnnyQt6vVEkQ1AcN9twhPG1b5j5UhoC",
        "FQ3ePUyLt2UvuQCw2nBKG8LGLvXHAZPzpdD6QQAAv9pW",
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        "4Av8W98PVbeYcdaw6T3L6STDdL5p3MWrqU7RkB8os4i7",
        "8c3zk1t1qt3RU43ckuvPkCS7HLbjJqq3J3Me8ov4aHrp",
        "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
        "8yn5oqFMwYA8SgGqWwKq1Hia8aM5gh1DWmHEL34hMqBX",
        "4gbnMK8Yv5ySJ4aFFTR9EFf3nY2GKbDwd9U1zh41inX9",
        "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",
        "Ah32VqAKNiE68qP35aPwxUbS15AuMo6d37ppyexR1CVe",
        "8LfiSAHzkKFsPCkwqYu11YtyhdN5dHTyDZc5trujPubg",
        "FoRGERiW7odcCBGU1bztZi16osPBHjxharvDathL5eds",
        "FahUFnmJDwVEcsRkrEmN2sZyEyjijQAwRfm7A4RCRou7",
        "5CUdD64Zktf2KJyHpaEKMzAVrNwG2saYX4FSsCheuLdA",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress": "GqPnSDXwp4JFtKS7YZ2HERgBbYLKpKVYy9TpVunzLRa9",
        "displayOptions": {
            "showFungible": true
        }
    }
    "#;

    let request: api::GetAssetsByOwner = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_owner(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_by_owner_without_show_fungible() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "Ff3Ci7EnM6KUdNDtzZvCjC5APgz1ajYW2Pt2BfT6Wftq",
        "7mX5Gzny3r8WBRnnyQt6vVEkQ1AcN9twhPG1b5j5UhoC",
        "FQ3ePUyLt2UvuQCw2nBKG8LGLvXHAZPzpdD6QQAAv9pW",
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        "4Av8W98PVbeYcdaw6T3L6STDdL5p3MWrqU7RkB8os4i7",
        "8c3zk1t1qt3RU43ckuvPkCS7HLbjJqq3J3Me8ov4aHrp",
        "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
        "8yn5oqFMwYA8SgGqWwKq1Hia8aM5gh1DWmHEL34hMqBX",
        "4gbnMK8Yv5ySJ4aFFTR9EFf3nY2GKbDwd9U1zh41inX9",
        "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",
        "Ah32VqAKNiE68qP35aPwxUbS15AuMo6d37ppyexR1CVe",
        "8LfiSAHzkKFsPCkwqYu11YtyhdN5dHTyDZc5trujPubg",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress": "GqPnSDXwp4JFtKS7YZ2HERgBbYLKpKVYy9TpVunzLRa9"
    }
    "#;

    let request: api::GetAssetsByOwner = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_owner(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_with_show_inscription_scenario_1() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "9FkS3kZV4MoGps14tUSp7iVnizGbxcK4bDEhSoF5oYAZ",
        "HMixBLSkuhiGgVbcGhqJar476xzu1bC8wM7yHsc1iXwP",
        "DarH4z6SmdVzPrt8krAygpLodhdjvNAstP3taj2tysN2",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "9FkS3kZV4MoGps14tUSp7iVnizGbxcK4bDEhSoF5oYAZ",
        "displayOptions": {
            "showInscription": true
        }
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_show_zero_balance_filter_set_to_true() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "BE1CkzRjLTXAWcSVCaqzycwXsZ18Yuk3jMDMnPUoHjjS",
        "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress":"2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
        "displayOptions": {
            "showZeroBalance": true
        }
    }
    "#;

    let request: api::GetTokenAccounts = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_token_accounts(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_show_zero_balance_filter_being_set_to_false() {
    let name = trim_test_name(function_name!());

    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "BE1CkzRjLTXAWcSVCaqzycwXsZ18Yuk3jMDMnPUoHjjS",
        "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ownerAddress":"2oerfxddTpK5hWAmCMYB6fr9WvNrjEH54CHCWK8sAq7g",
        "displayOptions": {
            "showZeroBalance": false
        }
    }
    "#;

    let request: api::GetTokenAccounts = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_token_accounts(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}
