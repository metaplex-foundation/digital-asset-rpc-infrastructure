use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_nft_editions() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Mainnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "Ey2Qb8kLctbchQsMnhZs5DjY32To2QtPuXNwWvk4NosL",
        "9ZmY7qCaq7WbrR7RZdHWCNS9FrFRPwRqU84wzWfmqLDz",
        "8SHfqzJYABeGfiG1apwiEYt6TvfGQiL1pdwEjvTKsyiZ",
        "GJvFDcBWf6aDncd1TBzx2ou1rgLFYaMBdbYLBa9oTAEw",
        "9ZmY7qCaq7WbrR7RZdHWCNS9FrFRPwRqU84wzWfmqLDz",
        "AoxgzXKEsJmUyF5pBb3djn9cJFA26zh2SQHvd9EYijZV",
        "9yQecKKYSHxez7fFjJkUvkz42TLmkoXzhyZxEf2pw8pz",
        "4V9QuYLpiMu4ZQmhdEHmgATdgiHkDeJfvZi84BfkYcez",
        "giWoA4jqHFkodPJgtbRYRcYtiXbsVytnxnEao3QT2gg",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "mintAddress": "Ey2Qb8kLctbchQsMnhZs5DjY32To2QtPuXNwWvk4NosL"
    }
    "#;

    let request: api::GetNftEditions = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_nft_editions(request).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}
