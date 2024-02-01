use function_name::named;
use std::str::FromStr;

use das_api::api::{self, ApiContract};
use digital_asset_types::dao::asset_creators;
use digital_asset_types::rpc::filter::{AssetSortBy, AssetSortDirection, AssetSorting};
use digital_asset_types::rpc::options::Options;
use itertools::Itertools;
use migration::sea_orm::{ConnectionTrait, EntityTrait};

use mpl_token_metadata::pda::find_metadata_account;

use mpl_token_metadata::state::Creator;
use sea_orm::{DbBackend, QueryTrait, Set};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;

use super::common::*;

#[tokio::test]
#[serial]
#[named]
async fn test_asset_parsing() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;
    apply_migrations_and_delete_data(setup.db.clone()).await;
    let mint: Pubkey = Pubkey::try_from("843gdpsTE4DoJz3ZoBsEjAqT8UgAcyF5YojygGgGZE1f").unwrap();
    index_nft(&setup, mint).await;
    let request = api::GetAsset {
        id: mint.to_string(),
        ..api::GetAsset::default()
    };
    let response = setup.das_api.get_asset(request).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}

#[tokio::test]
#[serial]
#[named]
async fn test_creators_reordering() {
    // This test covers a failure scenario we found in production where an NFT changed
    // the positions of its creators, leading to conflict errors in the DB.
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;
    let asset_id = "ANt9HygtvFmFJ1UcAHFLnM62JJWjk8fujMzjGfpKBfzk";
    let asset_pubkey = Pubkey::from_str(asset_id).unwrap();
    apply_migrations_and_delete_data(setup.db.clone()).await;

    // Insert the original creators
    let original_creators = vec![
        Creator {
            address: Pubkey::from_str("8Jhy62JeG4rgPu4Q2tn3Q3eZ8XUZmHhYDKpVJkQ8RFhe").unwrap(),
            verified: true,
            share: 0,
        },
        Creator {
            address: Pubkey::from_str("9sJ3GKyTpBaNJ9CVFV6DecV556G1jU9L32kJASxzWsQA").unwrap(),
            verified: false,
            share: 10,
        },
        Creator {
            address: Pubkey::from_str("yX9uyojU5uwBnDUJg5wX1n7T4w7KyU6r9brszXX2yKa").unwrap(),
            verified: false,
            share: 10,
        },
        Creator {
            address: Pubkey::from_str("BDaobvsTU8Eu3R4sx1vLufKiToaZL3MDTHxPgHgvGWC7").unwrap(),
            verified: false,
            share: 10,
        },
        Creator {
            address: Pubkey::from_str("F9xfmpggwgqH7ASZzNre8TxZztZwCogPBE8aQCNBLkBn").unwrap(),
            verified: false,
            share: 70,
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, c)| asset_creators::ActiveModel {
        asset_id: Set(asset_pubkey.clone().to_bytes().to_vec()),
        position: Set(i as i16),
        creator: Set(c.address.to_bytes().to_vec()),
        share: Set(c.share as i32),
        verified: Set(c.verified),
        slot_updated: Set(Some(0)),
        seq: Set(Some(0)),
        ..Default::default()
    })
    .collect::<Vec<_>>();
    setup
        .db
        .execute(asset_creators::Entity::insert_many(original_creators).build(DbBackend::Postgres))
        .await
        .unwrap();

    // Index the current NFT.
    index_nft(&setup, asset_pubkey).await;

    // Verify creators
    let request = api::GetAsset {
        id: asset_id.to_string(),
        ..api::GetAsset::default()
    };
    let response = setup.das_api.get_asset(request.clone()).await.unwrap();
    insta::assert_json_snapshot!(name, response);
}
