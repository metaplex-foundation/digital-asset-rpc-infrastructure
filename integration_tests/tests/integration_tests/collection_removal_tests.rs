use das_api::api::{self, ApiContract};
use function_name::named;
use itertools::Itertools;
use plerkle_serialization::{
    root_as_account_info, serializer::serialize_account,
    solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;

use super::common::*;

// ---------------------------------------------------------------------------
// Devnet mpl-core assets created by:
//   NEW_SCRIPTS/mpl-core/clients/js/create-collection-removal-test-assets.ts
// ---------------------------------------------------------------------------
const COLLECTION: &str = "3SGtviz5v4646TX5mwbgzsoqXgGW8DcSaBPPzWvyqUK5";
const ASSET_IN_COLLECTION_1: &str = "2w98yzs35WvGUiTvHNrUXZbVSgF2Fp1QgoYjstELhhFM";
const ASSET_IN_COLLECTION_2: &str = "FZTh4XVjWEM2pQw54faJ59oQJKhDbNv89GtjZDvSj5rj";
const STANDALONE_ASSET: &str = "dSMRsKXttKhs38bYWQzinmZ5t9apH1a8BkuSaTf5jvf";

// Second devnet mpl-core collection for re-assignment test.
// Created by an updated version of the same script above.
// TODO: Replace with real addresses after running the script.
const COLLECTION_B: &str = "9A4XhWZHdP53m9qGNSQmf5Kba4sQ7XCLW4uZrEu5j2S2";
const ASSET_IN_COLLECTION_B: &str = "HGqBXv6VEQiTmzUAaddrECesyQVXcVmz4zrdM4tydWsW";

// ---------------------------------------------------------------------------
// Devnet Token Metadata NFTs with verified collection, created by:
//   mpl-token-metadata/clients/js/create-devnet-collection-nft.ts
//
// TODO: Replace with real addresses after running the script.
// ---------------------------------------------------------------------------
const TM_COLLECTION_MINT: &str = "FaFYizXaqjWqHSgRSBdwSt8ee1R7MhvRrYebZVQZLPyJ";
const TM_NFT_MINT: &str = "6YidxdwZRjA8zdXNp4hXTUTGH7eiLBKox6oAHKRmgVwH";
const TM_NFT_MINT_2: &str = "8V2JKybMgkzgxocYdiWXcFg9fbVhtUNTrN5YCyuVJvsd";

/// Feed the raw account data of `data_source` to the transformer as though
/// it belongs to `target_pubkey`, at the given slot.  This lets us simulate
/// an account changing state (e.g. losing its collection) by swapping in
/// data from a different devnet account.
async fn index_account_data_as(
    setup: &TestSetup,
    target_pubkey: Pubkey,
    data_source: Pubkey,
    slot: u64,
) {
    let source_bytes = cached_fetch_account(setup, data_source, Some(DEFAULT_SLOT)).await;
    let source_info = root_as_account_info(&source_bytes).unwrap();
    let source_data: Vec<u8> = source_info.data().unwrap().iter().collect();

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &target_pubkey.to_bytes(),
        lamports: source_info.lamports(),
        owner: source_info.owner().unwrap().0.as_ref(),
        executable: source_info.executable(),
        rent_epoch: source_info.rent_epoch(),
        data: &source_data,
        write_version: 0,
        txn_signature: None,
    };
    let fbb = serialize_account(fbb, &account_info, slot, false);
    index_account_bytes(setup, fbb.finished_data().to_vec()).await;
}

// ---------------------------------------------------------------------------
// Test 1: Core asset removed from collection
//
// Index asset-1 (in a collection) at slot 1, then re-index the same pubkey
// with standalone-asset data at slot 2.  getAsset should no longer show the
// collection in its grouping.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_core_asset_removed_from_collection() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let collection_pk = Pubkey::try_from(COLLECTION).unwrap();
    let asset1_pk = Pubkey::try_from(ASSET_IN_COLLECTION_1).unwrap();
    let standalone_pk = Pubkey::try_from(STANDALONE_ASSET).unwrap();

    // Index the collection so the grouping foreign-key exists.
    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Index asset-1 at slot 1 — it is in the collection.
    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;

    // Verify the collection appears in getAsset.
    let request = api::GetAsset {
        id: asset1_pk.to_string(),
        ..api::GetAsset::default()
    };
    let response = setup.das_api.get_asset(request.clone()).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    assert!(
        grouping.iter().any(
            |g| g.group_key == "collection" && g.group_value == Some(collection_pk.to_string())
        ),
        "asset should be in collection before removal"
    );

    // Re-index the same pubkey with standalone data at slot 2 — simulates
    // the asset losing its collection.
    index_account_data_as(&setup, asset1_pk, standalone_pk, DEFAULT_SLOT + 1).await;

    // Verify the collection is gone.
    let response = setup.das_api.get_asset(request).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    assert!(
        !grouping.iter().any(
            |g| g.group_key == "collection" && g.group_value == Some(collection_pk.to_string())
        ),
        "collection should be removed after re-indexing with standalone data"
    );
}

// ---------------------------------------------------------------------------
// Test 2: getGrouping count after one asset leaves a collection
//
// Index a collection with two assets, then remove one.  getGrouping should
// report size = 1, not 2.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_get_grouping_count_after_collection_removal() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let collection_pk = Pubkey::try_from(COLLECTION).unwrap();
    let asset1_pk = Pubkey::try_from(ASSET_IN_COLLECTION_1).unwrap();
    let asset2_pk = Pubkey::try_from(ASSET_IN_COLLECTION_2).unwrap();
    let standalone_pk = Pubkey::try_from(STANDALONE_ASSET).unwrap();

    // Index collection + both assets at slot 1.
    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;
    index_account_data_as(&setup, asset2_pk, asset2_pk, DEFAULT_SLOT).await;

    // Verify grouping size is 2.
    let grouping_request = api::GetGrouping {
        group_key: "collection".to_string(),
        group_value: collection_pk.to_string(),
    };
    let grouping = setup
        .das_api
        .get_grouping(grouping_request.clone())
        .await
        .unwrap();
    assert_eq!(
        grouping.group_size, 2,
        "collection should have 2 members initially"
    );

    // Remove asset-1 from the collection (re-index with standalone data at
    // a higher slot).
    index_account_data_as(&setup, asset1_pk, standalone_pk, DEFAULT_SLOT + 1).await;

    // Verify grouping size dropped to 1.
    let grouping = setup.das_api.get_grouping(grouping_request).await.unwrap();
    assert_eq!(
        grouping.group_size, 1,
        "collection should have 1 member after removal"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Slot-gate idempotency — stale account update does not overwrite
//
// Remove asset from collection at slot 2, then replay the "in collection"
// state at slot 1.  The lower slot should be rejected, leaving the asset
// removed.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_slot_gate_rejects_stale_collection_update() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let collection_pk = Pubkey::try_from(COLLECTION).unwrap();
    let asset1_pk = Pubkey::try_from(ASSET_IN_COLLECTION_1).unwrap();
    let standalone_pk = Pubkey::try_from(STANDALONE_ASSET).unwrap();

    // Index the collection so the grouping FK exists.
    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Index asset-1 in collection at slot 1.
    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;

    // Remove it at slot 2.
    index_account_data_as(&setup, asset1_pk, standalone_pk, DEFAULT_SLOT + 1).await;

    // Replay the old "in collection" state at slot 1 (stale / out-of-order).
    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;

    // The removal at slot 2 should still win.
    let request = api::GetAsset {
        id: asset1_pk.to_string(),
        ..api::GetAsset::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    assert!(
        !grouping.iter().any(
            |g| g.group_key == "collection" && g.group_value == Some(collection_pk.to_string())
        ),
        "stale slot-1 replay must not restore the collection removed at slot 2"
    );
}

// ---------------------------------------------------------------------------
// Test 4: getAssetsByGroup excludes removed asset
//
// Index two assets in a collection, remove one, then call getAssetsByGroup.
// The removed asset must not appear in the results.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_get_assets_by_group_excludes_removed() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let collection_pk = Pubkey::try_from(COLLECTION).unwrap();
    let asset1_pk = Pubkey::try_from(ASSET_IN_COLLECTION_1).unwrap();
    let asset2_pk = Pubkey::try_from(ASSET_IN_COLLECTION_2).unwrap();
    let standalone_pk = Pubkey::try_from(STANDALONE_ASSET).unwrap();

    // Index collection + both assets at slot 1.
    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;
    index_account_data_as(&setup, asset2_pk, asset2_pk, DEFAULT_SLOT).await;

    // Remove asset-1.
    index_account_data_as(&setup, asset1_pk, standalone_pk, DEFAULT_SLOT + 1).await;

    let response = setup
        .das_api
        .get_assets_by_group(api::GetAssetsByGroup {
            group_key: "collection".to_string(),
            group_value: collection_pk.to_string(),
            sort_by: None,
            limit: Some(10),
            page: Some(1),
            before: None,
            after: None,
            options: None,
            cursor: None,
        })
        .await
        .unwrap();

    let ids: Vec<&str> = response.items.iter().map(|a| a.id.as_str()).collect();

    assert!(
        !ids.contains(&asset1_pk.to_string().as_str()),
        "removed asset must not appear in getAssetsByGroup"
    );
    assert!(
        ids.contains(&asset2_pk.to_string().as_str()),
        "remaining asset must still appear in getAssetsByGroup"
    );
}

// ---------------------------------------------------------------------------
// Test 5: Collection re-assignment (move asset from collection A → B)
//
// Index an asset in collection A, then re-index it with data from an asset
// in collection B at a higher slot.  Verify it appears in B's grouping and
// not A's, and both group counts are correct.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_collection_reassignment() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let collection_a = Pubkey::try_from(COLLECTION).unwrap();
    let collection_b = Pubkey::try_from(COLLECTION_B).unwrap();
    let asset1_pk = Pubkey::try_from(ASSET_IN_COLLECTION_1).unwrap();
    let asset2_pk = Pubkey::try_from(ASSET_IN_COLLECTION_2).unwrap();
    let asset_in_b_pk = Pubkey::try_from(ASSET_IN_COLLECTION_B).unwrap();

    // Index both collections.
    let seeds: Vec<SeedEvent> = seed_accounts([COLLECTION, COLLECTION_B]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Index two assets in collection A, one in collection B at slot 1.
    index_account_data_as(&setup, asset1_pk, asset1_pk, DEFAULT_SLOT).await;
    index_account_data_as(&setup, asset2_pk, asset2_pk, DEFAULT_SLOT).await;
    index_account_data_as(&setup, asset_in_b_pk, asset_in_b_pk, DEFAULT_SLOT).await;

    // Verify initial group counts.
    let grp_a = api::GetGrouping {
        group_key: "collection".to_string(),
        group_value: collection_a.to_string(),
    };
    let grp_b = api::GetGrouping {
        group_key: "collection".to_string(),
        group_value: collection_b.to_string(),
    };
    assert_eq!(
        setup
            .das_api
            .get_grouping(grp_a.clone())
            .await
            .unwrap()
            .group_size,
        2,
        "collection A should start with 2 members"
    );
    assert_eq!(
        setup
            .das_api
            .get_grouping(grp_b.clone())
            .await
            .unwrap()
            .group_size,
        1,
        "collection B should start with 1 member"
    );

    // Move asset-1 from A → B by re-indexing with collection-B asset data.
    index_account_data_as(&setup, asset1_pk, asset_in_b_pk, DEFAULT_SLOT + 1).await;

    // Verify getAsset shows collection B.
    let request = api::GetAsset {
        id: asset1_pk.to_string(),
        ..api::GetAsset::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    assert!(
        grouping.iter().any(
            |g| g.group_key == "collection" && g.group_value == Some(collection_b.to_string())
        ),
        "asset should now be in collection B"
    );
    assert!(
        !grouping.iter().any(
            |g| g.group_key == "collection" && g.group_value == Some(collection_a.to_string())
        ),
        "asset should no longer be in collection A"
    );

    // Verify updated group counts.
    assert_eq!(
        setup.das_api.get_grouping(grp_a).await.unwrap().group_size,
        1,
        "collection A should have 1 member after reassignment"
    );
    assert_eq!(
        setup.das_api.get_grouping(grp_b).await.unwrap().group_size,
        2,
        "collection B should have 2 members after reassignment"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Token Metadata asset with collection cleared
//
// Uses a real mainnet NFT.  Fetch its metadata, clear the collection field,
// re-serialize, and feed at a higher slot.  getAsset should no longer show
// the collection.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_token_metadata_collection_cleared() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let mint = Pubkey::try_from(TM_NFT_MINT).unwrap();
    let metadata_pubkey = mpl_token_metadata::accounts::Metadata::find_pda(&mint).0;

    apply_migrations_and_delete_data(setup.db.clone()).await;

    // Index the full NFT (mint, metadata, token) from devnet.
    index_nft(&setup, mint).await;

    let request = api::GetAsset {
        id: mint.to_string(),
        ..api::GetAsset::default()
    };

    // Verify asset currently has a collection.
    let response = setup.das_api.get_asset(request.clone()).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    let has_collection = grouping
        .iter()
        .any(|g| g.group_key == "collection" && g.group_value.is_some());
    assert!(has_collection, "NFT should have a collection initially");

    // Fetch the metadata account, clear the collection, re-index at higher slot.
    let metadata_bytes = cached_fetch_account(&setup, metadata_pubkey, None).await;
    let metadata_info = root_as_account_info(&metadata_bytes).unwrap();
    let metadata_data: Vec<u8> = metadata_info.data().unwrap().iter().collect();

    let mut metadata = mpl_token_metadata::accounts::Metadata::from_bytes(&metadata_data).unwrap();

    // Clear the collection.
    metadata.collection = None;

    let modified_data = borsh::to_vec(&metadata).unwrap();

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &metadata_info.pubkey().unwrap().0,
        lamports: metadata_info.lamports(),
        owner: metadata_info.owner().unwrap().0.as_ref(),
        executable: metadata_info.executable(),
        rent_epoch: metadata_info.rent_epoch(),
        data: &modified_data,
        write_version: 0,
        txn_signature: None,
    };
    let fbb = serialize_account(fbb, &account_info, DEFAULT_SLOT + 1, false);
    index_account_bytes(&setup, fbb.finished_data().to_vec()).await;

    // Verify collection is gone.
    let response = setup.das_api.get_asset(request).await.unwrap();
    let grouping = response.grouping.as_deref().unwrap_or_default();
    let has_collection = grouping
        .iter()
        .any(|g| g.group_key == "collection" && g.group_value.is_some());
    assert!(
        !has_collection,
        "collection should be removed after clearing metadata"
    );
}

// ---------------------------------------------------------------------------
// Test 7: getGrouping count for Token Metadata after collection removal
//
// Index two TM NFTs in the same collection, clear the collection on one,
// verify getGrouping count drops from 2 to 1.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_tm_get_grouping_count_after_collection_removal() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let mint1 = Pubkey::try_from(TM_NFT_MINT).unwrap();
    let mint2 = Pubkey::try_from(TM_NFT_MINT_2).unwrap();
    let metadata1 = mpl_token_metadata::accounts::Metadata::find_pda(&mint1).0;
    let collection_mint = Pubkey::try_from(TM_COLLECTION_MINT).unwrap();

    apply_migrations_and_delete_data(setup.db.clone()).await;

    // Index both NFTs.
    index_nft(&setup, mint1).await;
    index_nft(&setup, mint2).await;

    let grouping_request = api::GetGrouping {
        group_key: "collection".to_string(),
        group_value: collection_mint.to_string(),
    };

    // Verify initial count is 2.
    let grouping = setup
        .das_api
        .get_grouping(grouping_request.clone())
        .await
        .unwrap();
    assert_eq!(
        grouping.group_size, 2,
        "TM collection should have 2 members initially"
    );

    // Clear collection on NFT 1.
    let metadata_bytes = cached_fetch_account(&setup, metadata1, None).await;
    let metadata_info = root_as_account_info(&metadata_bytes).unwrap();
    let metadata_data: Vec<u8> = metadata_info.data().unwrap().iter().collect();

    let mut metadata = mpl_token_metadata::accounts::Metadata::from_bytes(&metadata_data).unwrap();
    metadata.collection = None;

    let modified_data = borsh::to_vec(&metadata).unwrap();

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &metadata_info.pubkey().unwrap().0,
        lamports: metadata_info.lamports(),
        owner: metadata_info.owner().unwrap().0.as_ref(),
        executable: metadata_info.executable(),
        rent_epoch: metadata_info.rent_epoch(),
        data: &modified_data,
        write_version: 0,
        txn_signature: None,
    };
    let fbb = serialize_account(fbb, &account_info, DEFAULT_SLOT + 1, false);
    index_account_bytes(&setup, fbb.finished_data().to_vec()).await;

    // Verify count dropped to 1.
    let grouping = setup.das_api.get_grouping(grouping_request).await.unwrap();
    assert_eq!(
        grouping.group_size, 1,
        "TM collection should have 1 member after removal"
    );
}
