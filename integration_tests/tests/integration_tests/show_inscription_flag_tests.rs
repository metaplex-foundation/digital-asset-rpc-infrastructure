use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

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
