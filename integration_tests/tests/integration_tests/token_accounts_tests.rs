use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_token_accounts() {
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
