use borsh::{BorshDeserialize, BorshSerialize};
use das_api::api::{self, ApiContract};
use digital_asset_types::rpc::{Asset, Interface};
use function_name::named;
use itertools::Itertools;
use plerkle_serialization::{
    root_as_account_info, serializer::serialize_account,
    solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use super::common::{
    apply_migrations_and_delete_data, cached_fetch_account, index_account_bytes, index_seed_events,
    seed_accounts, trim_test_name, Network, SeedEvent, TestSetup, TestSetupOptions, DEFAULT_SLOT,
};

const INDEXED_QUERY_TEMPLATE_ACCOUNT: &str = "DHciVfQxHHM7t2asQJRjjkKbjvZ4PuG3Y3uiULMQUjJQ";
const STALE_FILTER_TEMPLATE_ACCOUNT: &str = "JChzyyp1CnNz56tJLteQ5BsbngmWQ3JwcxLZrmuQA5b7";
const SLOT_GUARD_TEMPLATE_ACCOUNT: &str = "Do7rVGmVNa9wjsKNyjoa5phqriLER6HCqUQm5zyoTX3f";
const APPEND_HISTORY_TEMPLATE_ACCOUNT: &str = "41thppJ4z9HnBNbFMLnztXS7seqBptYV1jG8UhxR4vK8";
const MULTI_MATCH_TEMPLATE_ACCOUNT_A: &str = "53q1PCBy5KgzZfoHu6bnLWQFVmJtKyceP8DqNMhXWUaA";
const MULTI_MATCH_TEMPLATE_ACCOUNT_B: &str = "Hvdg2FjMEndC4jxF2MJgKCaj5omLLZ19LNfD4p9oXkpE";
const NESTED_GROUP_TEMPLATE_ACCOUNT_A: &str = "9CSyGBw1DCVZfx621nb7UBM9SpVDsX1m9MaN6APCf1Ci";
const NESTED_GROUP_TEMPLATE_ACCOUNT_B: &str = "5SW7HyPZx2vsVnA2vxxzAMZi51Tkc5v2LkXTxzx8ULAm";

#[derive(Clone, Debug)]
enum GroupAccountUpdate {
    ParentGroups(Vec<Pubkey>),
    ParentGroupsAndUri {
        parent_groups: Vec<Pubkey>,
        uri: String,
    },
}

#[derive(Clone, Debug, BorshDeserialize)]
struct TestCollectionV1AccountData {
    _key: u8,
    update_authority: Pubkey,
    name: String,
    uri: String,
    _num_minted: u32,
    _current_size: u32,
}

#[derive(Clone, Debug, BorshSerialize)]
struct TestGroupV1AccountData {
    key: u8,
    update_authority: Pubkey,
    name: String,
    uri: String,
    collections: Vec<Pubkey>,
    groups: Vec<Pubkey>,
    parent_groups: Vec<Pubkey>,
    assets: Vec<Pubkey>,
}

fn parse_collection_account(address: &str) -> Pubkey {
    Pubkey::from_str(address).unwrap()
}

fn build_group_account_data_from_collection(
    template_account: Pubkey,
    account_data: &[u8],
    update: GroupAccountUpdate,
) -> Vec<u8> {
    let mut account_data_slice = account_data;
    let collection = TestCollectionV1AccountData::deserialize(&mut account_data_slice)
        .unwrap_or_else(|err| {
        panic!(
            "The base account {} is not a valid CollectionV1 account: {}",
            template_account, err
        )
    });

    let (parent_groups, uri) = match update {
        GroupAccountUpdate::ParentGroups(parent_groups) => (parent_groups, collection.uri),
        GroupAccountUpdate::ParentGroupsAndUri { parent_groups, uri } => (parent_groups, uri),
    };

    let group = TestGroupV1AccountData {
        key: 6,
        update_authority: collection.update_authority,
        name: collection.name,
        uri,
        collections: vec![],
        groups: vec![],
        parent_groups,
        assets: vec![],
    };

    group.try_to_vec().unwrap()
}

async fn index_group_account_update(
    setup: &TestSetup,
    template_account: Pubkey,
    group_account: Pubkey,
    update: GroupAccountUpdate,
    slot: u64,
) {
    let account_bytes = cached_fetch_account(setup, template_account, Some(DEFAULT_SLOT)).await;
    let account_info = root_as_account_info(&account_bytes).unwrap();
    let account_data = account_info.data().unwrap().iter().collect::<Vec<_>>();
    let modified_account_data =
        build_group_account_data_from_collection(template_account, &account_data, update);
    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_bytes = group_account.to_bytes();
    let account_update = ReplicaAccountInfoV2 {
        pubkey: &account_bytes,
        lamports: account_info.lamports(),
        owner: account_info.owner().unwrap().0.as_ref(),
        executable: account_info.executable(),
        rent_epoch: account_info.rent_epoch(),
        data: &modified_account_data,
        write_version: 0,
        txn_signature: None,
    };
    let is_startup = false;
    let fbb = serialize_account(fbb, &account_update, slot, is_startup);
    let account_bytes = fbb.finished_data().to_vec();
    index_account_bytes(setup, account_bytes).await;
}

fn extract_group_values(asset: &Asset) -> Vec<String> {
    let mut values = asset
        .grouping
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|group| group.group_key == "group")
        .filter_map(|group| group.group_value)
        .collect::<Vec<_>>();
    values.sort();
    values
}

async fn seed_template_accounts(setup: &TestSetup, accounts: &[&str]) {
    let seeds: Vec<SeedEvent> = seed_accounts(accounts.iter().copied());
    index_seed_events(setup, seeds.iter().collect_vec()).await;
}

async fn get_assets_by_group_ids(setup: &TestSetup, parent_group: Pubkey) -> Vec<String> {
    let by_group = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "group".to_string(),
            group_value: parent_group.to_string(),
            sort_by: None,
            limit: Some(50),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    by_group
        .items
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>()
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_is_indexed_and_queryable_via_das() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[INDEXED_QUERY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(INDEXED_QUERY_TEMPLATE_ACCOUNT);
    let group_account = template_account;
    let parent_a = Pubkey::new_unique();
    let parent_b = Pubkey::new_unique();
    let mut expected_group_values = vec![parent_a.to_string(), parent_b.to_string()];
    expected_group_values.sort();

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![parent_a, parent_b]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();

    assert_eq!(asset.id, group_account.to_string());
    assert_eq!(asset.interface, Interface::MplCoreGroup);
    assert_eq!(extract_group_values(&asset), expected_group_values);

    let by_group = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "group".to_string(),
            group_value: parent_a.to_string(),
            sort_by: None,
            limit: Some(50),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    assert!(by_group.items.iter().any(|item| item.id == group_account.to_string()));
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_stale_filtering_returns_most_recent_relationships() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[STALE_FILTER_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(STALE_FILTER_TEMPLATE_ACCOUNT);
    let group_account = template_account;
    let parent_a = Pubkey::new_unique();
    let parent_b = Pubkey::new_unique();
    let mut expected_group_values = vec![parent_a.to_string(), parent_b.to_string()];
    expected_group_values.sort();

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![parent_a, parent_b]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let initial_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(initial_asset.interface, Interface::MplCoreGroup);
    assert_eq!(extract_group_values(&initial_asset), expected_group_values);

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![]),
        DEFAULT_SLOT + 2,
    )
    .await;

    let updated_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(updated_asset.interface, Interface::MplCoreGroup);
    assert!(extract_group_values(&updated_asset).is_empty());
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_older_slot_update_does_not_override_newer_state() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[SLOT_GUARD_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(SLOT_GUARD_TEMPLATE_ACCOUNT);
    let group_account = template_account;
    let newer_parent = Pubkey::new_unique();
    let older_parent = Pubkey::new_unique();
    let newer_slot = DEFAULT_SLOT + 3;
    let older_slot = DEFAULT_SLOT + 2;
    let newer_uri = "https://example.com/newer-group.json".to_string();
    let older_uri = "https://example.com/older-group.json".to_string();

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroupsAndUri {
            parent_groups: vec![newer_parent],
            uri: newer_uri.clone(),
        },
        newer_slot,
    )
    .await;

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroupsAndUri {
            parent_groups: vec![older_parent],
            uri: older_uri,
        },
        older_slot,
    )
    .await;

    let asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();

    assert_eq!(asset.interface, Interface::MplCoreGroup);
    assert_eq!(extract_group_values(&asset), vec![newer_parent.to_string()]);
    assert_eq!(
        asset.content.as_ref().map(|content| content.json_uri.clone()),
        Some(newer_uri)
    );
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_query_uses_append_history_while_asset_view_filters_stale() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[APPEND_HISTORY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(APPEND_HISTORY_TEMPLATE_ACCOUNT);
    let group_account = Pubkey::new_unique();
    let stale_parent = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![stale_parent]),
        DEFAULT_SLOT + 1,
    )
    .await;

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![]),
        DEFAULT_SLOT + 2,
    )
    .await;

    let by_group = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "group".to_string(),
            group_value: stale_parent.to_string(),
            sort_by: None,
            limit: Some(50),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    assert!(
        by_group.items.iter().any(|item| item.id == group_account.to_string()),
        "group query should still include append-history relations"
    );

    let asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(asset.interface, Interface::MplCoreGroup);
    assert!(
        extract_group_values(&asset).is_empty(),
        "asset view should filter stale group relations and expose latest empty relationship state"
    );
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_query_returns_multiple_matching_accounts() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(
        &setup,
        &[MULTI_MATCH_TEMPLATE_ACCOUNT_A, MULTI_MATCH_TEMPLATE_ACCOUNT_B],
    )
    .await;

    let first_template_account = parse_collection_account(MULTI_MATCH_TEMPLATE_ACCOUNT_A);
    let second_template_account = parse_collection_account(MULTI_MATCH_TEMPLATE_ACCOUNT_B);
    let first_group_account = Pubkey::new_unique();
    let second_group_account = Pubkey::new_unique();
    let shared_parent = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        first_template_account,
        first_group_account,
        GroupAccountUpdate::ParentGroups(vec![shared_parent, Pubkey::new_unique()]),
        DEFAULT_SLOT + 1,
    )
    .await;

    index_group_account_update(
        &setup,
        second_template_account,
        second_group_account,
        GroupAccountUpdate::ParentGroups(vec![shared_parent]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let by_group = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "group".to_string(),
            group_value: shared_parent.to_string(),
            sort_by: None,
            limit: Some(50),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    let ids = by_group
        .items
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>();

    assert!(ids.contains(&first_group_account.to_string()));
    assert!(ids.contains(&second_group_account.to_string()));
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_nested_group_relation_is_queryable() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(
        &setup,
        &[NESTED_GROUP_TEMPLATE_ACCOUNT_A, NESTED_GROUP_TEMPLATE_ACCOUNT_B],
    )
    .await;

    let parent_template_account = parse_collection_account(NESTED_GROUP_TEMPLATE_ACCOUNT_A);
    let child_template_account = parse_collection_account(NESTED_GROUP_TEMPLATE_ACCOUNT_B);
    let parent_group_account = Pubkey::new_unique();
    let child_group_account = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        parent_template_account,
        parent_group_account,
        GroupAccountUpdate::ParentGroups(vec![]),
        DEFAULT_SLOT + 1,
    )
    .await;

    index_group_account_update(
        &setup,
        child_template_account,
        child_group_account,
        GroupAccountUpdate::ParentGroups(vec![parent_group_account]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let by_group = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "group".to_string(),
            group_value: parent_group_account.to_string(),
            sort_by: None,
            limit: Some(50),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    let ids = by_group
        .items
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>();
    assert!(ids.contains(&child_group_account.to_string()));
    assert!(!ids.contains(&parent_group_account.to_string()));

    let child_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: child_group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(child_asset.interface, Interface::MplCoreGroup);
    assert_eq!(
        extract_group_values(&child_asset),
        vec![parent_group_account.to_string()]
    );
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_is_included_in_non_fungible_search_results() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[INDEXED_QUERY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(INDEXED_QUERY_TEMPLATE_ACCOUNT);
    let group_account = template_account;
    let parent = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        template_account,
        group_account,
        GroupAccountUpdate::ParentGroups(vec![parent]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let group_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();

    let owner_address = group_asset
        .ownership
        .as_ref()
        .map(|ownership| ownership.owner.clone())
        .filter(|owner| !owner.is_empty())
        .expect("group owner should be present in indexed asset");

    let request = format!(
        r#"{{
        "ownerAddress": "{}",
        "page": 1,
        "limit": 50,
        "tokenType": "NonFungible"
        }}"#,
        owner_address
    );
    let request: api::SearchAssets = serde_json::from_str(&request).unwrap();
    let response = setup.das_api.search_assets(request).await.unwrap();
    assert!(
        response
            .items
            .iter()
            .any(|item| item.id == group_account.to_string() && item.interface == Interface::MplCoreGroup),
        "non-fungible search should return indexed group assets with MplCoreGroup interface"
    );
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_child_with_multiple_parents_is_queryable_from_each_parent() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[INDEXED_QUERY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(INDEXED_QUERY_TEMPLATE_ACCOUNT);
    let child_group_account = Pubkey::new_unique();
    let parent_a = Pubkey::new_unique();
    let parent_b = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        template_account,
        child_group_account,
        GroupAccountUpdate::ParentGroups(vec![parent_a, parent_b]),
        DEFAULT_SLOT + 1,
    )
    .await;

    let ids_for_parent_a = get_assets_by_group_ids(&setup, parent_a).await;
    let ids_for_parent_b = get_assets_by_group_ids(&setup, parent_b).await;

    assert!(ids_for_parent_a.contains(&child_group_account.to_string()));
    assert!(ids_for_parent_b.contains(&child_group_account.to_string()));

    let child_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: child_group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(child_asset.interface, Interface::MplCoreGroup);

    let mut expected_groups = vec![parent_a.to_string(), parent_b.to_string()];
    expected_groups.sort();
    assert_eq!(extract_group_values(&child_asset), expected_groups);
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_multilevel_links_are_indexed_per_direct_parent_child_edge() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[INDEXED_QUERY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(INDEXED_QUERY_TEMPLATE_ACCOUNT);
    let parent_group_account = Pubkey::new_unique();
    let child_group_account = Pubkey::new_unique();
    let grandchild_group_account = Pubkey::new_unique();
    let slot = DEFAULT_SLOT + 1;

    index_group_account_update(
        &setup,
        template_account,
        parent_group_account,
        GroupAccountUpdate::ParentGroups(vec![]),
        slot,
    )
    .await;
    index_group_account_update(
        &setup,
        template_account,
        child_group_account,
        GroupAccountUpdate::ParentGroups(vec![parent_group_account]),
        slot,
    )
    .await;
    index_group_account_update(
        &setup,
        template_account,
        grandchild_group_account,
        GroupAccountUpdate::ParentGroups(vec![child_group_account]),
        slot,
    )
    .await;

    let ids_for_parent = get_assets_by_group_ids(&setup, parent_group_account).await;
    let ids_for_child = get_assets_by_group_ids(&setup, child_group_account).await;
    let ids_for_grandchild = get_assets_by_group_ids(&setup, grandchild_group_account).await;

    assert!(ids_for_parent.contains(&child_group_account.to_string()));
    assert!(!ids_for_parent.contains(&grandchild_group_account.to_string()));
    assert!(ids_for_child.contains(&grandchild_group_account.to_string()));
    assert!(!ids_for_child.contains(&parent_group_account.to_string()));
    assert!(ids_for_grandchild.is_empty());

    let child_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: child_group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(child_asset.interface, Interface::MplCoreGroup);
    assert_eq!(
        extract_group_values(&child_asset),
        vec![parent_group_account.to_string()]
    );

    let grandchild_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: grandchild_group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(grandchild_asset.interface, Interface::MplCoreGroup);
    assert_eq!(
        extract_group_values(&grandchild_asset),
        vec![child_group_account.to_string()]
    );
}

#[tokio::test]
#[serial]
#[named]
async fn test_group_reparenting_keeps_append_history_queries_and_latest_asset_view() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    seed_template_accounts(&setup, &[INDEXED_QUERY_TEMPLATE_ACCOUNT]).await;

    let template_account = parse_collection_account(INDEXED_QUERY_TEMPLATE_ACCOUNT);
    let child_group_account = Pubkey::new_unique();
    let old_parent = Pubkey::new_unique();
    let new_parent = Pubkey::new_unique();

    index_group_account_update(
        &setup,
        template_account,
        child_group_account,
        GroupAccountUpdate::ParentGroups(vec![old_parent]),
        DEFAULT_SLOT + 1,
    )
    .await;
    index_group_account_update(
        &setup,
        template_account,
        child_group_account,
        GroupAccountUpdate::ParentGroups(vec![new_parent]),
        DEFAULT_SLOT + 2,
    )
    .await;

    let ids_for_old_parent = get_assets_by_group_ids(&setup, old_parent).await;
    let ids_for_new_parent = get_assets_by_group_ids(&setup, new_parent).await;

    assert!(ids_for_old_parent.contains(&child_group_account.to_string()));
    assert!(ids_for_new_parent.contains(&child_group_account.to_string()));

    let child_asset = setup
        .das_api
        .get_asset(api::GetAsset {
            id: child_group_account.to_string(),
            ..api::GetAsset::default()
        })
        .await
        .unwrap();
    assert_eq!(child_asset.interface, Interface::MplCoreGroup);
    assert_eq!(
        extract_group_values(&child_asset),
        vec![new_parent.to_string()]
    );
}
