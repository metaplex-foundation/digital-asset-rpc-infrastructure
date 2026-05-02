use das_api::api::{self, ApiContract};
use function_name::named;
use itertools::Itertools;
use serial_test::serial;

use super::common::*;

// Real devnet GroupV1, Collection, and Asset accounts created by
// mpl-core/clients/js/create-devnet-group-assets.ts with wallet
// 2VqRJUywvWrNd6bhoZ3CZUsqXGgTtWCTbfvpdUWnXHT2.

/// A bare GroupV1 with no relationships.
const STANDALONE_GROUP: &str = "LGgLVqsvxrJGbkkGrC4uadbXLVvhNEUQesPzPFT1StL";

/// A GroupV1 that has one child collection.
const GROUP_WITH_COLLECTION: &str = "9JwY1rksJdUZ5AJNrzS56fJy8D5pvHbjuSzoLS1SAfG1";
/// The CollectionV1 that is a child of GROUP_WITH_COLLECTION (has Groups plugin).
const COLLECTION_IN_GROUP: &str = "Ge8yEpxv2KEZDKCbGUhdD8h5v2M377Ep9H94aR5DPKd1";

/// A GroupV1 that has one child asset.
const GROUP_WITH_ASSET: &str = "56RUh47zGh7iYZRq9QzptfwTJhtLsmYbbZDNknVwAgqQ";
/// The AssetV1 that is a child of GROUP_WITH_ASSET (has Groups plugin).
const ASSET_IN_GROUP: &str = "9XBVndadik4nt25UnBhhMQqGzVMXbcYy3dmWLeAkFMVb";

/// A GroupV1 that is the parent of CHILD_GROUP.
const PARENT_GROUP: &str = "GXemjec273GC6fXTMmCB7yguPxRMhKed16mZNDghkbKJ";
/// A GroupV1 that is a child of PARENT_GROUP.
const CHILD_GROUP: &str = "GpYTJ84neoZMM6ZvGRMHRyLZZgjciqfVzUiPNTnzw3XN";

/// A GroupV1 that has a child collection which itself contains an asset.
const GROUP_WITH_COLLECTION_AND_ASSET: &str = "GqMg7pu1hjKJ8qK237ArjLw9rxwuP1aZDkUhNAr6Fxoq";
/// The CollectionV1 child of GROUP_WITH_COLLECTION_AND_ASSET (has Groups plugin).
const COLLECTION_IN_GROUP_WITH_ASSET: &str = "BBAEBZeNsDx8mgMAHSkLKzC8SPhqVJWGr9cGAk33TRTE";
/// The AssetV1 that belongs to COLLECTION_IN_GROUP_WITH_ASSET.
const ASSET_IN_COLLECTION_IN_GROUP: &str = "CHWtWeBtW9SsccMsxdAL6APrRH4BzppR6EYP8TjK1ZLP";

/// Cyclic pair: A is parent of B and B is parent of A.
const CYCLIC_GROUP_A: &str = "E58qZkkNA1w2iVdsJZEG7f2iQF9ASxdpKT1vPUacA5Vb";
const CYCLIC_GROUP_B: &str = "2cmLRHqhakcocHN9MvvZL9Qq6qPK6t1MfnHqpxeUu85Q";

// ---------------------------------------------------------------------------
// get_asset: verify each account type indexes correctly
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_group_standalone() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([STANDALONE_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{STANDALONE_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_group_with_collection() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([GROUP_WITH_COLLECTION]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{GROUP_WITH_COLLECTION}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_collection_in_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{COLLECTION_IN_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_group_with_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([GROUP_WITH_ASSET]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{GROUP_WITH_ASSET}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset_in_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([ASSET_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{ASSET_IN_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_parent_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([PARENT_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{PARENT_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_child_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([CHILD_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{CHILD_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_collection_in_group_with_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        GROUP_WITH_COLLECTION_AND_ASSET,
        COLLECTION_IN_GROUP_WITH_ASSET,
    ]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset = serde_json::from_str(&format!(
        r#"{{ "id": "{COLLECTION_IN_GROUP_WITH_ASSET}" }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_asset_in_collection_in_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> =
        seed_accounts([COLLECTION_IN_GROUP_WITH_ASSET, ASSET_IN_COLLECTION_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{ASSET_IN_COLLECTION_IN_GROUP}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

// ---------------------------------------------------------------------------
// get_assets_by_group: verify group queries work
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_group_with_collection() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([GROUP_WITH_COLLECTION, COLLECTION_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{GROUP_WITH_COLLECTION}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_group_with_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([GROUP_WITH_ASSET, ASSET_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{GROUP_WITH_ASSET}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_parent_child_groups() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([PARENT_GROUP, CHILD_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{PARENT_GROUP}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_group_with_collection_and_asset() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([
        GROUP_WITH_COLLECTION_AND_ASSET,
        COLLECTION_IN_GROUP_WITH_ASSET,
        ASSET_IN_COLLECTION_IN_GROUP,
    ]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{GROUP_WITH_COLLECTION_AND_ASSET}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

// ---------------------------------------------------------------------------
// get_assets_by_group with "collection" key still works for collection-in-group
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_collection_for_collection_in_group() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> =
        seed_accounts([COLLECTION_IN_GROUP_WITH_ASSET, ASSET_IN_COLLECTION_IN_GROUP]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "collection",
            "groupValue": "{COLLECTION_IN_GROUP_WITH_ASSET}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

// ---------------------------------------------------------------------------
// Cyclic groups: A is parent of B, B is parent of A
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_cyclic_group_a() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([CYCLIC_GROUP_A, CYCLIC_GROUP_B]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{CYCLIC_GROUP_A}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_cyclic_group_b() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([CYCLIC_GROUP_A, CYCLIC_GROUP_B]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAsset =
        serde_json::from_str(&format!(r#"{{ "id": "{CYCLIC_GROUP_B}" }}"#)).unwrap();
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_cyclic_group_a() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([CYCLIC_GROUP_A, CYCLIC_GROUP_B]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{CYCLIC_GROUP_A}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_mpl_core_get_assets_by_group_for_cyclic_group_b() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let seeds: Vec<SeedEvent> = seed_accounts([CYCLIC_GROUP_A, CYCLIC_GROUP_B]);
    apply_migrations_and_delete_data(setup.db.clone()).await;
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request: api::GetAssetsByGroup = serde_json::from_str(&format!(
        r#"{{
            "groupKey": "group",
            "groupValue": "{CYCLIC_GROUP_B}",
            "sortBy": {{ "sortBy": "updated", "sortDirection": "asc" }},
            "page": 1,
            "limit": 50
        }}"#
    ))
    .unwrap();
    let response = setup.das_api.get_assets_by_group(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
