use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_with_show_fungible_scenario_1() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "Ca84nWhQu41DMRnjdhRrLZty1i9txepMhAhz5qLLGcBw",
        "7z6b5TE4WX4mgcQjuNBTDxK4SE75sbgEg5WWJwoUeie8",
        "8myaCN6KcKVkMqroXuLJq6QsqRcPbvme4wV5Ubfr5mDC",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "Ca84nWhQu41DMRnjdhRrLZty1i9txepMhAhz5qLLGcBw",
        "displayOptions": {
            "showFungible": true
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
async fn test_get_asset_with_show_fungible_scenario_2() {
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
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "8Xv3SpX94HHf32Apg4TeSeS3i2p6wuXeE8FBZr168Hti",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
        "displayOptions": {
            "showFungible": true
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
async fn test_get_asset_with_show_fungible_scenario_3() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts(["2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
        "displayOptions": {
            "showFungible": true
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
async fn test_get_asset_with_show_fungible_scenario_4() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
        "8yn5oqFMwYA8SgGqWwKq1Hia8aM5gh1DWmHEL34hMqBX",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn",
        "displayOptions": {
            "showFungible": true
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
        "AH6wj7T8Ke5nbukjtcobjjs1CDWUcQxndtnLkKAdrSrM",
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "8Xv3SpX94HHf32Apg4TeSeS3i2p6wuXeE8FBZr168Hti",
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
async fn test_get_asset_by_authority_with_show_fungible() {
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
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "8Xv3SpX94HHf32Apg4TeSeS3i2p6wuXeE8FBZr168Hti",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "authorityAddress": "2RtGg6fsFiiF1EQzHqbd66AhW7R5bWeQGpTbv2UMkCdW",
        "displayOptions": {
            "showFungible": true
        }
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
async fn test_get_asset_by_creator_with_show_fungible() {
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
        "7fXKY9tPpvYsdbSNyesUqo27WYC6ZsBEULdtngGHqLCK",
        "8Xv3SpX94HHf32Apg4TeSeS3i2p6wuXeE8FBZr168Hti",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "creatorAddress": "5XvhfmRjwXkGp3jHGmaKpqeerNYjkuZZBYLVQYdeVcRv",
        "displayOptions": {
            "showFungible": true
        }
    }
    "#;

    let request: api::GetAssetsByCreator = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_creator(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}
