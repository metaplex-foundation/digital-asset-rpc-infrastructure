#[cfg(test)]
mod common;

use sea_orm::{
    entity::prelude::*, Condition, DatabaseBackend, JoinType, MockDatabase, QuerySelect,
};
use solana_sdk::{signature::Keypair, signer::Signer};

use blockbuster::token_metadata::state::*;
use common::*;
use digital_asset_types::dao::sea_orm_active_enums::*;
use digital_asset_types::dao::{
    asset, asset_authority, asset_creators, asset_data,
    sea_orm_active_enums::{OwnerType, RoyaltyTargetType},
};

#[tokio::test]
async fn get_assets_by_creator() -> Result<(), DbErr> {
    let id_1 = Keypair::new().pubkey();
    let owner_1 = Keypair::new().pubkey();
    let update_authority_1 = Keypair::new().pubkey();
    let creator_1 = Keypair::new().pubkey();
    let uri_1 = Keypair::new().pubkey();

    let id_2 = Keypair::new().pubkey();
    let owner_2 = Keypair::new().pubkey();
    let update_authority_2 = Keypair::new().pubkey();
    let creator_2 = Keypair::new().pubkey();
    let uri_2 = Keypair::new().pubkey();

    let id_3 = Keypair::new().pubkey();
    let update_authority_3 = Keypair::new().pubkey();
    let creator_3 = Keypair::new().pubkey();
    let uri_3 = Keypair::new().pubkey();

    let metadata_1 = MockMetadataArgs {
        name: String::from("Test #1"),
        symbol: String::from("BUBBLE"),
        uri: uri_1.to_string(),
        primary_sale_happened: true,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![Creator {
            address: creator_1,
            share: 100,
            verified: true,
        }]
        .to_vec(),
        seller_fee_basis_points: 100,
    };

    let asset_data_1 = create_asset_data(metadata_1.clone(), id_1.to_bytes().to_vec());
    let asset_1 = create_asset(
        id_1.to_bytes().to_vec(),
        owner_1.to_bytes().to_vec(),
        OwnerType::Single,
        None,
        false,
        1,
        None,
        true,
        false,
        None,
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        metadata_1.seller_fee_basis_points as i32,
    );

    let asset_creator_1_1 = create_asset_creator(
        id_1.to_bytes().to_vec(),
        metadata_1.creators[0].address.to_bytes().to_vec(),
        100,
        true,
        1,
    );

    let asset_authority_1 = create_asset_authority(
        id_1.to_bytes().to_vec(),
        update_authority_1.to_bytes().to_vec(),
        1,
    );

    let metadata_2 = MockMetadataArgs {
        name: String::from("Test #2"),
        symbol: String::from("BUBBLE"),
        uri: uri_2.to_string(),
        primary_sale_happened: true,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![Creator {
            address: creator_2,
            share: 100,
            verified: true,
        }]
        .to_vec(),
        seller_fee_basis_points: 100,
    };

    let asset_data_2 = create_asset_data(metadata_2.clone(), id_2.to_bytes().to_vec());
    let asset_2 = create_asset(
        id_2.to_bytes().to_vec(),
        owner_2.to_bytes().to_vec(),
        OwnerType::Single,
        None,
        false,
        1,
        None,
        true,
        false,
        None,
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        metadata_2.seller_fee_basis_points as i32,
    );

    let asset_creator_2_1 = create_asset_creator(
        id_2.to_bytes().to_vec(),
        metadata_2.creators[0].address.to_bytes().to_vec(),
        100,
        true,
        2,
    );

    let asset_authority_2 = create_asset_authority(
        id_2.to_bytes().to_vec(),
        update_authority_2.to_bytes().to_vec(),
        2,
    );

    let metadata_3 = MockMetadataArgs {
        name: String::from("Test #3"),
        symbol: String::from("BUBBLE"),
        uri: uri_3.to_string(),
        primary_sale_happened: true,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![
            Creator {
                address: creator_2,
                share: 10,
                verified: true,
            },
            Creator {
                address: creator_3,
                share: 90,
                verified: true,
            },
        ]
        .to_vec(),
        seller_fee_basis_points: 100,
    };

    let asset_data_3 = create_asset_data(metadata_3.clone(), id_3.to_bytes().to_vec());
    let asset_3 = create_asset(
        id_3.to_bytes().to_vec(),
        owner_2.to_bytes().to_vec(),
        OwnerType::Single,
        None,
        false,
        1,
        None,
        true,
        false,
        None,
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        metadata_3.seller_fee_basis_points as i32,
    );

    let asset_creator_3_1 = create_asset_creator(
        id_3.to_bytes().to_vec(),
        metadata_3.creators[0].address.to_bytes().to_vec(),
        10,
        true,
        3,
    );

    let asset_creator_3_2 = create_asset_creator(
        id_3.to_bytes().to_vec(),
        metadata_3.creators[1].address.to_bytes().to_vec(),
        90,
        true,
        4,
    );

    let asset_authority_3 = create_asset_authority(
        id_3.to_bytes().to_vec(),
        update_authority_3.to_bytes().to_vec(),
        3,
    );

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![asset_data_1.1]])
        .append_query_results(vec![vec![asset_1.1]])
        .append_query_results(vec![vec![asset_creator_1_1.1]])
        .append_query_results(vec![vec![asset_authority_1.1]])
        .append_query_results(vec![vec![asset_data_2.1.clone()]])
        .append_query_results(vec![vec![asset_2.1.clone()]])
        .append_query_results(vec![vec![asset_creator_2_1.1]])
        .append_query_results(vec![vec![asset_authority_2.1]])
        .append_query_results(vec![vec![asset_data_3.1.clone()]])
        .append_query_results(vec![vec![asset_3.1.clone()]])
        .append_query_results(vec![vec![asset_creator_3_1.1]])
        .append_query_results(vec![vec![asset_creator_3_2.1]])
        .append_query_results(vec![vec![asset_authority_3.1]])
        .append_query_results(vec![vec![
            (asset_2.1.clone(), asset_data_2.1.clone()),
            (asset_3.1.clone(), asset_data_3.1.clone()),
        ]])
        .into_connection();

    let _insert_result = asset_data::Entity::insert(asset_data_1.0)
        .exec(&db)
        .await
        .unwrap();

    let insert_result = asset::Entity::insert(asset_1.0).exec(&db).await.unwrap();
    assert_eq!(insert_result.last_insert_id, id_1.to_bytes().to_vec());

    let _insert_result = asset_creators::Entity::insert(asset_creator_1_1.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_authority::Entity::insert(asset_authority_1.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_data::Entity::insert(asset_data_2.0)
        .exec(&db)
        .await
        .unwrap();

    let insert_result = asset::Entity::insert(asset_2.0).exec(&db).await.unwrap();
    assert_eq!(insert_result.last_insert_id, id_2.to_bytes().to_vec());

    let _insert_result = asset_creators::Entity::insert(asset_creator_2_1.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_authority::Entity::insert(asset_authority_2.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_data::Entity::insert(asset_data_3.0)
        .exec(&db)
        .await
        .unwrap();

    let insert_result = asset::Entity::insert(asset_3.0).exec(&db).await.unwrap();
    assert_eq!(insert_result.last_insert_id, id_3.to_bytes().to_vec());

    let _insert_result = asset_creators::Entity::insert(asset_creator_3_1.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_creators::Entity::insert(asset_creator_3_2.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_authority::Entity::insert(asset_authority_3.0)
        .exec(&db)
        .await
        .unwrap();

    assert_eq!(
        asset::Entity::find()
            .join(
                JoinType::LeftJoin,
                asset::Entity::has_many(asset_creators::Entity).into(),
            )
            .filter(
                Condition::any()
                    .add(asset_creators::Column::Creator.eq(creator_2.to_bytes().to_vec())),
            )
            .find_also_related(asset_data::Entity)
            .all(&db)
            .await?,
        vec![
            (asset_2.1.clone(), Some(asset_data_2.1.clone())),
            (asset_3.1.clone(), Some(asset_data_3.1.clone())),
        ]
    );

    Ok(())
}
