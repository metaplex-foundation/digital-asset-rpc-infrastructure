use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_show_zero_balance_filter_being_enabled() {
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
    "CyqarC6hyNYvb3EDueyeYrnGeAUjCDtMvWrbtdAnA53a"
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
async fn test_show_zero_balance_filter_being_disabled() {
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
    "CyqarC6hyNYvb3EDueyeYrnGeAUjCDtMvWrbtdAnA53a"
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
