use das_api::api::{self, ApiContract};
use digital_asset_types::dao::asset;
use function_name::named;
use itertools::Itertools;
use plerkle_serialization::{
    serializer::serialize_account, solana_geyser_plugin_interface_shims::ReplicaAccountInfoV2,
};
use sea_orm::{entity::*, sea_query::Expr, EntityTrait, QueryFilter};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use super::common::*;

// AgentIdentity PDA layout constants (mirrors mpl-agent-identity on-chain layout).
const KEY_AGENT_IDENTITY_V1: u8 = 1;
const KEY_AGENT_IDENTITY_V2: u8 = 2;
const AGENT_IDENTITY_V1_LEN: usize = 40;
const AGENT_IDENTITY_V2_LEN: usize = 104;
const ASSET_PUBKEY_OFFSET: usize = 8;
const ASSET_PUBKEY_END: usize = ASSET_PUBKEY_OFFSET + 32;
const AGENT_TOKEN_MINT_OFFSET: usize = ASSET_PUBKEY_END;
const AGENT_TOKEN_MINT_END: usize = AGENT_TOKEN_MINT_OFFSET + 32;

// ---------------------------------------------------------------------------
// Devnet Core asset WITH the AgentIdentity external plugin + AgentIdentityV2 PDA.
// Created by: mpl-agent/clients/js/create-agent-test-assets.ts
// (registerIdentityV1 adds the plugin AND creates the PDA)
// ---------------------------------------------------------------------------
const AGENT_CORE_ASSET: &str = "84jw9dw7hMRJXFvzJXrBzVQpmVWaGUtYT7R6QhNU9qt3";
const AGENT_IDENTITY_PDA: &str = "J6xFz2thno6pwBVmbFPdBcQDEReWBMU4nLmhsS5SBAt6";

// Devnet Core asset WITHOUT the AgentIdentity plugin (plain standalone).
// Reuses the standalone asset from collection removal tests.
const PLAIN_CORE_ASSET: &str = "dSMRsKXttKhs38bYWQzinmZ5t9apH1a8BkuSaTf5jvf";

// Devnet Token Metadata NFT (non-Core).
// Reuses the TM NFT from collection removal tests.
const TM_NFT_MINT: &str = "6YidxdwZRjA8zdXNp4hXTUTGH7eiLBKox6oAHKRmgVwH";

// Agent registry program ID.
const AGENT_REGISTRY_PROGRAM: &str = "1DREGFgysWYxLnRnKQnwrxnJQeSMk2HmGaC6whw2B2p";

// mpl-core program ID for asset_signer PDA derivation.
const MPL_CORE_PROGRAM: &str = "CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d";

// Deterministic fake token mint for fabricated agent registry PDA tests.
const FAKE_TOKEN_MINT: &str = "FakeToken11111111111111111111111111111111111";

/// Derive the expected asset_signer PDA for a Core asset.
fn derive_asset_signer(asset_pubkey: &Pubkey) -> Pubkey {
    let mpl_core = Pubkey::from_str(MPL_CORE_PROGRAM).unwrap();
    let (pda, _) =
        Pubkey::find_program_address(&[b"mpl-core-execute", asset_pubkey.as_ref()], &mpl_core);
    pda
}

/// Build a fabricated Agent Registry PDA account (AgentIdentityV2, 104 bytes)
/// and feed it to the transformer.
async fn index_fabricated_agent_registry_v2(
    setup: &TestSetup,
    asset_pubkey: &Pubkey,
    agent_token_mint: Option<&Pubkey>,
    slot: u64,
) {
    let agent_registry_program = Pubkey::from_str(AGENT_REGISTRY_PROGRAM).unwrap();

    let (pda, bump) = Pubkey::find_program_address(
        &[b"agent_identity", asset_pubkey.as_ref()],
        &agent_registry_program,
    );

    let mut data = vec![0u8; AGENT_IDENTITY_V2_LEN];
    data[0] = KEY_AGENT_IDENTITY_V2;
    data[1] = bump;
    data[ASSET_PUBKEY_OFFSET..ASSET_PUBKEY_END].copy_from_slice(asset_pubkey.as_ref());
    if let Some(mint) = agent_token_mint {
        data[AGENT_TOKEN_MINT_OFFSET..AGENT_TOKEN_MINT_END].copy_from_slice(mint.as_ref());
    }

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &pda.to_bytes(),
        lamports: 1_000_000,
        owner: &agent_registry_program.to_bytes(),
        executable: false,
        rent_epoch: 0,
        data: &data,
        write_version: 0,
        txn_signature: None,
    };
    let fbb = serialize_account(fbb, &account_info, slot, false);
    index_account_bytes(setup, fbb.finished_data().to_vec()).await;
}

/// Build a fabricated Agent Registry PDA account (AgentIdentityV1, 40 bytes)
/// and feed it to the transformer.
async fn index_fabricated_agent_registry_v1(setup: &TestSetup, asset_pubkey: &Pubkey, slot: u64) {
    let agent_registry_program = Pubkey::from_str(AGENT_REGISTRY_PROGRAM).unwrap();

    let (pda, bump) = Pubkey::find_program_address(
        &[b"agent_identity", asset_pubkey.as_ref()],
        &agent_registry_program,
    );

    let mut data = vec![0u8; AGENT_IDENTITY_V1_LEN];
    data[0] = KEY_AGENT_IDENTITY_V1;
    data[1] = bump;
    data[ASSET_PUBKEY_OFFSET..ASSET_PUBKEY_END].copy_from_slice(asset_pubkey.as_ref());

    let fbb = flatbuffers::FlatBufferBuilder::new();
    let account_info = ReplicaAccountInfoV2 {
        pubkey: &pda.to_bytes(),
        lamports: 1_000_000,
        owner: &agent_registry_program.to_bytes(),
        executable: false,
        rent_epoch: 0,
        data: &data,
        write_version: 0,
        txn_signature: None,
    };
    let fbb = serialize_account(fbb, &account_info, slot, false);
    index_account_bytes(setup, fbb.finished_data().to_vec()).await;
}

// ---------------------------------------------------------------------------
// Test 1: Core asset WITH AgentIdentity plugin
//
// Indexing a Core asset that has the AgentIdentity external plugin should
// produce is_agent=true and a valid asset_signer PDA in the getAsset response.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_core_asset_with_agent_identity() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(AGENT_CORE_ASSET).unwrap();

    let seeds: Vec<SeedEvent> = seed_accounts([AGENT_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = api::GetAsset {
        id: asset_pk.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert_eq!(
        response.is_agent,
        Some(true),
        "Core asset with AgentIdentity plugin must have is_agent=true"
    );

    let expected_signer = derive_asset_signer(&asset_pk);
    assert_eq!(
        response.asset_signer,
        Some(expected_signer.to_string()),
        "asset_signer must be the correct PDA"
    );

    assert!(
        response.agent_token.is_none(),
        "agent_token must be absent when no AgentIdentityV2 PDA has been indexed"
    );

    insta::assert_json_snapshot!(name, response);
}

// ---------------------------------------------------------------------------
// Test 2: Core asset WITHOUT AgentIdentity plugin
//
// A plain Core asset should have is_agent=false and asset_signer present,
// but agent_token absent.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_core_asset_without_agent_identity() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(PLAIN_CORE_ASSET).unwrap();

    let seeds: Vec<SeedEvent> = seed_accounts([PLAIN_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = api::GetAsset {
        id: asset_pk.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert_eq!(
        response.is_agent,
        Some(false),
        "Core asset without AgentIdentity plugin must have is_agent=false"
    );

    let expected_signer = derive_asset_signer(&asset_pk);
    assert_eq!(
        response.asset_signer,
        Some(expected_signer.to_string()),
        "asset_signer must still be present for any Core asset"
    );

    assert!(response.agent_token.is_none(), "agent_token must be absent");
}

// ---------------------------------------------------------------------------
// Test 3: Non-Core asset (Token Metadata NFT)
//
// is_agent, agent_token, and asset_signer should all be omitted (None)
// from the response for non-Core assets.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_non_core_asset_omits_agent_fields() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let seeds: Vec<SeedEvent> = seed_nfts([TM_NFT_MINT]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    let request = api::GetAsset {
        id: TM_NFT_MINT.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert!(
        response.is_agent.is_none(),
        "Non-Core asset must not include is_agent"
    );
    assert!(
        response.agent_token.is_none(),
        "Non-Core asset must not include agent_token"
    );
    assert!(
        response.asset_signer.is_none(),
        "Non-Core asset must not include asset_signer"
    );
}

// ---------------------------------------------------------------------------
// Test 4: AgentIdentityV2 PDA with token mint
//
// After indexing an agent Core asset and then a fabricated AgentIdentityV2 PDA
// that contains a token mint, the getAsset response should include
// the agent_token field alongside is_agent=true and asset_signer.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_agent_identity_v2_with_token_mint() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(AGENT_CORE_ASSET).unwrap();

    // Index the agent Core asset first so the row exists (has AgentIdentity plugin).
    let seeds: Vec<SeedEvent> = seed_accounts([AGENT_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Fabricate an AgentIdentityV2 PDA with a token mint.
    let fake_token_mint = Pubkey::from_str(FAKE_TOKEN_MINT).unwrap();
    index_fabricated_agent_registry_v2(&setup, &asset_pk, Some(&fake_token_mint), DEFAULT_SLOT + 1)
        .await;

    let request = api::GetAsset {
        id: asset_pk.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert_eq!(
        response.is_agent,
        Some(true),
        "agent asset must have is_agent=true"
    );
    assert_eq!(
        response.agent_token,
        Some(fake_token_mint.to_string()),
        "agent_token must match the mint from the AgentIdentityV2 PDA"
    );

    insta::assert_json_snapshot!(name, response);
}

// ---------------------------------------------------------------------------
// Test 5a: Real AgentIdentityV2 PDA (no token mint set)
//
// The PDA created by registerIdentityV1 starts with no token mint.
// After indexing the real PDA, agent_token should remain null.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_agent_identity_v2_no_token() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(AGENT_CORE_ASSET).unwrap();

    // Index the Core asset first.
    let seeds: Vec<SeedEvent> = seed_accounts([AGENT_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Index the real AgentIdentityV2 PDA (no token mint set).
    let pda_pk = Pubkey::from_str(AGENT_IDENTITY_PDA).unwrap();
    index_account(&setup, pda_pk).await;

    let request = api::GetAsset {
        id: asset_pk.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert!(
        response.agent_token.is_none(),
        "agent_token must remain null when V2 PDA has no token mint set"
    );
}

// ---------------------------------------------------------------------------
// Test 5b: Fabricated AgentIdentityV1 PDA (no token mint field)
//
// V1 has no token_mint field at all, so agent_token should remain null.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_agent_identity_v1_no_token() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(PLAIN_CORE_ASSET).unwrap();

    // Index the Core asset first.
    let seeds: Vec<SeedEvent> = seed_accounts([PLAIN_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Fabricate an AgentIdentityV1 PDA (no token mint).
    index_fabricated_agent_registry_v1(&setup, &asset_pk, DEFAULT_SLOT + 1).await;

    let request = api::GetAsset {
        id: asset_pk.to_string(),
        ..Default::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();

    assert!(
        response.agent_token.is_none(),
        "agent_token must remain null for AgentIdentityV1 (no token mint field)"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Burnt asset with agent registry PDA — no update applied
//
// If the Core asset is marked as burnt, a subsequent AgentIdentityV2 PDA
// update must be silently skipped (agent_token stays null).
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_burnt_asset_ignores_agent_registry() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(PLAIN_CORE_ASSET).unwrap();
    let asset_id = asset_pk.to_bytes().to_vec();

    // Index the Core asset.
    let seeds: Vec<SeedEvent> = seed_accounts([PLAIN_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Mark the asset as burnt directly in the DB.
    asset::Entity::update_many()
        .col_expr(asset::Column::Burnt, Expr::value(true))
        .filter(asset::Column::Id.eq(asset_id.clone()))
        .exec(setup.db.as_ref())
        .await
        .unwrap();

    // Now try to index an AgentIdentityV2 PDA with a token mint.
    let fake_token_mint = Pubkey::from_str(FAKE_TOKEN_MINT).unwrap();
    index_fabricated_agent_registry_v2(&setup, &asset_pk, Some(&fake_token_mint), DEFAULT_SLOT + 1)
        .await;

    // Query the DB directly to verify agent_token was NOT written.
    let row = asset::Entity::find_by_id(asset_id)
        .one(setup.db.as_ref())
        .await
        .unwrap()
        .expect("asset row must exist");

    assert!(
        row.agent_token.is_none(),
        "agent_token must not be set on a burnt asset"
    );
    assert!(
        row.slot_updated_agent_registry.is_none(),
        "slot_updated_agent_registry must not be set on a burnt asset"
    );
}

// ---------------------------------------------------------------------------
// Test 6b: Stale-slot agent registry update is ignored
//
// If an agent registry PDA is indexed at a higher slot and then a stale
// (lower-slot) update arrives, the newer data must be preserved.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_stale_slot_agent_registry_update_ignored() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let asset_pk = Pubkey::from_str(AGENT_CORE_ASSET).unwrap();
    let asset_id = asset_pk.to_bytes().to_vec();

    // Index the Core asset so the row exists.
    let seeds: Vec<SeedEvent> = seed_accounts([AGENT_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Apply a V2 agent registry update at a high slot with a token mint.
    let fake_token_mint = Pubkey::from_str(FAKE_TOKEN_MINT).unwrap();
    index_fabricated_agent_registry_v2(
        &setup,
        &asset_pk,
        Some(&fake_token_mint),
        DEFAULT_SLOT + 10,
    )
    .await;

    // Record the values written by the high-slot update.
    let row_before = asset::Entity::find_by_id(asset_id.clone())
        .one(setup.db.as_ref())
        .await
        .unwrap()
        .expect("asset row must exist");

    assert_eq!(
        row_before.agent_token,
        Some(fake_token_mint.to_bytes().to_vec()),
    );
    assert_eq!(
        row_before.slot_updated_agent_registry,
        Some(DEFAULT_SLOT as i64 + 10),
    );

    // Replay a stale V1 update at a lower slot (no token mint).
    index_fabricated_agent_registry_v1(&setup, &asset_pk, DEFAULT_SLOT + 1).await;

    // Verify the newer data was preserved.
    let row_after = asset::Entity::find_by_id(asset_id)
        .one(setup.db.as_ref())
        .await
        .unwrap()
        .expect("asset row must exist");

    assert_eq!(
        row_after.agent_token, row_before.agent_token,
        "stale update must not overwrite agent_token"
    );
    assert_eq!(
        row_after.slot_updated_agent_registry, row_before.slot_updated_agent_registry,
        "stale update must not overwrite slot_updated_agent_registry"
    );
}

// ---------------------------------------------------------------------------
// Test 7: searchAssets with isAgent, agentToken, assetSigner filters
//
// Index both an agent asset and a plain asset, then verify that the
// searchAssets filters correctly narrow results.
// ---------------------------------------------------------------------------
#[tokio::test]
#[serial]
#[named]
async fn test_search_assets_agent_filters() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name.clone(),
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;
    apply_migrations_and_delete_data(setup.db.clone()).await;

    let agent_pk = Pubkey::from_str(AGENT_CORE_ASSET).unwrap();
    let plain_pk = Pubkey::from_str(PLAIN_CORE_ASSET).unwrap();

    // Index both assets.
    let seeds: Vec<SeedEvent> = seed_accounts([AGENT_CORE_ASSET, PLAIN_CORE_ASSET]);
    index_seed_events(&setup, seeds.iter().collect_vec()).await;

    // Also add a fabricated V2 PDA with token mint for the agent asset.
    let fake_token_mint = Pubkey::from_str(FAKE_TOKEN_MINT).unwrap();
    index_fabricated_agent_registry_v2(&setup, &agent_pk, Some(&fake_token_mint), DEFAULT_SLOT + 1)
        .await;

    // --- Filter: isAgent = true ---
    let response = setup
        .das_api
        .search_assets(api::SearchAssets {
            is_agent: Some(true),
            page: Some(1),
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();

    let ids: Vec<&str> = response.items.iter().map(|a| a.id.as_str()).collect();
    assert!(
        ids.contains(&agent_pk.to_string().as_str()),
        "isAgent=true must include the agent asset"
    );
    assert!(
        !ids.contains(&plain_pk.to_string().as_str()),
        "isAgent=true must exclude the plain asset"
    );

    // --- Filter: isAgent = false ---
    let response = setup
        .das_api
        .search_assets(api::SearchAssets {
            is_agent: Some(false),
            page: Some(1),
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();

    let ids: Vec<&str> = response.items.iter().map(|a| a.id.as_str()).collect();
    assert!(
        ids.contains(&plain_pk.to_string().as_str()),
        "isAgent=false must include the plain asset"
    );
    assert!(
        !ids.contains(&agent_pk.to_string().as_str()),
        "isAgent=false must exclude the agent asset"
    );

    // --- Filter: agentToken ---
    let response = setup
        .das_api
        .search_assets(api::SearchAssets {
            agent_token: Some(fake_token_mint.to_string()),
            page: Some(1),
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();

    let ids: Vec<&str> = response.items.iter().map(|a| a.id.as_str()).collect();
    assert!(
        ids.contains(&agent_pk.to_string().as_str()),
        "agentToken filter must return the agent asset"
    );
    assert_eq!(
        ids.len(),
        1,
        "agentToken filter must return exactly one result"
    );

    // --- Filter: assetSigner ---
    let expected_signer = derive_asset_signer(&agent_pk);
    let response = setup
        .das_api
        .search_assets(api::SearchAssets {
            asset_signer: Some(expected_signer.to_string()),
            page: Some(1),
            limit: Some(10),
            ..Default::default()
        })
        .await
        .unwrap();

    let ids: Vec<&str> = response.items.iter().map(|a| a.id.as_str()).collect();
    assert!(
        ids.contains(&agent_pk.to_string().as_str()),
        "assetSigner filter must return the matching agent asset"
    );
    assert_eq!(
        ids.len(),
        1,
        "assetSigner filter must return exactly one result"
    );
}
