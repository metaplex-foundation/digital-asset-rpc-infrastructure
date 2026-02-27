use function_name::named;

use das_api::api::{self, ApiContract};

use itertools::Itertools;

use serial_test::serial;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_changes_basic() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts(["CMVuYDS9nTeujfTPJb8ik7CRhAqZv4DfjfdamFLkJgxE"]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = api::GetAssetChanges {
        limit: Some(10),
        ..Default::default()
    };
    let response = setup.das_api.get_asset_changes(request).await.unwrap();

    assert!(response.current_slot > 0);
    assert!(!response.items.is_empty());
    for item in &response.items {
        assert!(!item.id.is_empty());
        assert!(item.slot_updated > 0);
    }
    // With results present, after cursor should be set
    assert!(response.after.is_some());
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_changes_cursor_pagination() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([
        "CMVuYDS9nTeujfTPJb8ik7CRhAqZv4DfjfdamFLkJgxE",
        "HTKAVZZrDdyecCxzm3WEkCsG1GUmiqKm73PvngfuYRNK",
        "2NqdYX6kJmMUoChnDXU2UrP9BsoPZivRw3uJG8iDhRRd",
    ]);

    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Fetch first page with limit=1
    let request = api::GetAssetChanges {
        limit: Some(1),
        ..Default::default()
    };
    let page1 = setup.das_api.get_asset_changes(request).await.unwrap();
    assert_eq!(page1.items.len(), 1);
    assert!(page1.after.is_some());

    // Fetch second page using cursor
    let request = api::GetAssetChanges {
        limit: Some(1),
        after: page1.after.clone(),
        ..Default::default()
    };
    let page2 = setup.das_api.get_asset_changes(request).await.unwrap();
    assert_eq!(page2.items.len(), 1);

    // Pages should return different items
    assert_ne!(page1.items[0].id, page2.items[0].id);

    // Ordering: slot_updated should be non-decreasing across pages
    assert!(page2.items[0].slot_updated >= page1.items[0].slot_updated);
}

#[tokio::test]
#[serial]
#[named]
async fn test_get_asset_changes_empty() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    // Clean DB, no seeds
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let request = api::GetAssetChanges {
        limit: Some(10),
        ..Default::default()
    };
    let response = setup.das_api.get_asset_changes(request).await.unwrap();

    assert!(response.items.is_empty());
    assert!(response.after.is_none());
    assert_eq!(response.current_slot, 0);
}
