use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

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

    let seeds: Vec<SeedEvent> = seed_accounts(["AVyNtmNdLAbxyzPDbaeJjpVJSPb5vtyido8NzyKKuVjQ"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "AVyNtmNdLAbxyzPDbaeJjpVJSPb5vtyido8NzyKKuVjQ"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

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

    let seeds: Vec<SeedEvent> = seed_accounts(["JCPjxuL4abG7M7NDtKUt1ekh3jg1FJLW6n1G92TUpoA4"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "JCPjxuL4abG7M7NDtKUt1ekh3jg1FJLW6n1G92TUpoA4"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
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
        "6Wp9xk6GrD4EmDKuD7fr2URJubjtGq5MXENR9UU15C9i",
        "3YhJuW9X9Hvf4MVv5qP5xWxEMPrQu8uTwjkKR1q7D1gh",
        "79Npv5WTGGkfVc4QYhmnz9xRUbCPV4g5aNKvotvGo4Ko",
        "9qS8Xo1M3RUqvrLwd5RaA6iMbzh9An7nwDSDe1aB5mtr",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"
    {
        "groupKey": "collection",
        "groupValue": "6Wp9xk6GrD4EmDKuD7fr2URJubjtGq5MXENR9UU15C9i",
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
