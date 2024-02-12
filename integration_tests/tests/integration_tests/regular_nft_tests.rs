use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_reg_get_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts(["CMVuYDS9nTeujfTPJb8ik7CRhAqZv4DfjfdamFLkJgxE"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "id": "CMVuYDS9nTeujfTPJb8ik7CRhAqZv4DfjfdamFLkJgxE"
    }
    "#;

    let request: api::GetAsset = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_reg_get_asset_batch() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "HTKAVZZrDdyecCxzm3WEkCsG1GUmiqKm73PvngfuYRNK",
        "2NqdYX6kJmMUoChnDXU2UrP9BsoPZivRw3uJG8iDhRRd",
        "5rEeYv8R25b8j6YTHJvYuCKEzq44UCw1Wx1Wx2VPPLz1",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    for (request, individual_test_name) in [
        (
            r#"        
        {
            "ids": ["HTKAVZZrDdyecCxzm3WEkCsG1GUmiqKm73PvngfuYRNK", "2NqdYX6kJmMUoChnDXU2UrP9BsoPZivRw3uJG8iDhRRd"]
        }
        "#,
            "only-2",
        ),
        (
            r#"        
        {
            "ids": ["2NqdYX6kJmMUoChnDXU2UrP9BsoPZivRw3uJG8iDhRRd", "5rEeYv8R25b8j6YTHJvYuCKEzq44UCw1Wx1Wx2VPPLz1"]
        }
        "#,
            "only-2-different-2",
        ),
        (
            r#"        
        {
            "ids": [
                "2NqdYX6kJmMUoChnDXU2UrP9BsoPZivRw3uJG8iDhRRd",
                "JECLQnbo2CCL8Ygn6vTFn7yeKn8qc7i51bAa9BCAJnWG",
                "5rEeYv8R25b8j6YTHJvYuCKEzq44UCw1Wx1Wx2VPPLz1"
            ]
        }
        "#,
            "2-and-a-missing-1",
        ),
    ] {
        let request: api::GetAssets = serde_json::from_str(request).unwrap();
        let response = setup.das_api.get_assets(request).await.unwrap();
        insta::assert_json_snapshot!(format!("{}-{}", name, individual_test_name), response);
    }
}

#[tokio::test]
#[serial]
#[named]
async fn test_reg_get_asset_by_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "7jFuJ73mBPDdLMvCYxzrpFTD9FeDudRxdXGDALP5Cp2W",
        "BioVudBTjJnuDW22q62XPhGP87sVwZKcQ46MPSNz4gqi",
        "Fm9S3FL23z3ii3EBBv8ozqLninLvhWDYmcHcHaZy6nie",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "groupKey": "collection",
        "groupValue": "8Rt3Ayqth4DAiPnW9MDFi63TiQJHmohfTWLMQFHi4KZH",
        "sortBy": {
            "sortBy": "updated",
            "sortDirection": "asc"
        },
        "page": 1,
        "limit": 1
    }
    "#;

    let request: api::GetAssetsByGroup = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_reg_search_assets() {
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
