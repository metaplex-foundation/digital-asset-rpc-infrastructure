use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_assets_with_multiple_same_ids() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        "F9Lw3ki3hJ7PF9HQXsBzoY8GyE6sPoEZZdXJBsTTD2rk",
        "DZAZ3mGuq7nCYGzUyw4MiA74ysr15EfqLpzCzX2cRVng",
        "JEKKtnGvjiZ8GtATnMVgadHU41AuTbFkMW8oD2tdyV9X",
        "2ecGsTKbj7FecLwxTHaodZRFwza7m7LamqDG4YjczZMj",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = r#"        
    {
        "ids": [
          "F9Lw3ki3hJ7PF9HQXsBzoY8GyE6sPoEZZdXJBsTTD2rk",
          "F9Lw3ki3hJ7PF9HQXsBzoY8GyE6sPoEZZdXJBsTTD2rk",
          "JEKKtnGvjiZ8GtATnMVgadHU41AuTbFkMW8oD2tdyV9X",
          "JEKKtnGvjiZ8GtATnMVgadHU41AuTbFkMW8oD2tdyV9X"
        ]
    }
    "#;

    let request: api::GetAssets = serde_json::from_str(request).unwrap();
    let response = setup.das_api.get_assets(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
