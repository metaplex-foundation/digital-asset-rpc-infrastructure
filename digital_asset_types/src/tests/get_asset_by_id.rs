#[cfg(test)]
use sea_orm::{entity::prelude::*, DatabaseBackend, MockDatabase};
use solana_sdk::{signature::Keypair, signer::Signer};

use crate::{
    adapter::{Creator, TokenProgramVersion, TokenStandard},
    dao::{
        generated::prelude::AssetData,
        generated::sea_orm_active_enums::{OwnerType, RoyaltyTargetType},
        generated::{asset, asset_authority, asset_creators, asset_data},
    },
    tests::{
        create_asset, create_asset_authority, create_asset_creator, create_asset_data, MetadataArgs,
    },
};

#[async_std::test]
async fn get_asset_by_id() -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let update_authority = Keypair::new().pubkey();
    let creator_1 = Keypair::new().pubkey();
    let uri = Keypair::new().pubkey();

    let metadata_1 = MetadataArgs {
        name: String::from("Test #1"),
        symbol: String::from("BUBBLE"),
        uri: uri.to_string(),
        primary_sale_happened: true,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        token_program_version: TokenProgramVersion::Original,
        creators: vec![Creator {
            address: creator_1,
            share: 100,
            verified: true,
        }]
        .to_vec(),
        seller_fee_basis_points: 100,
    };

    let asset_data_1 = create_asset_data(metadata_1.clone(), 1);
    let asset_1 = create_asset(
        id.to_bytes().to_vec(),
        owner.to_bytes().to_vec(),
        OwnerType::Single,
        None,
        false,
        1,
        None,
        true,
        false,
        None,
        1,
        0 as i64,
        None,
        RoyaltyTargetType::Creators,
        None,
        metadata_1.seller_fee_basis_points as i32,
        Some(1),
    );

    let asset_creator_1_1 = create_asset_creator(
        id.to_bytes().to_vec(),
        metadata_1.creators[0].address.to_bytes().to_vec(),
        100,
        true,
        1,
    );

    let asset_authority_1 = create_asset_authority(
        id.to_bytes().to_vec(),
        update_authority.to_bytes().to_vec(),
        1,
    );

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![asset_data_1.1.clone()]])
        .append_query_results(vec![vec![asset_1.1.clone()]])
        .append_query_results(vec![vec![asset_creator_1_1.1]])
        .append_query_results(vec![vec![asset_authority_1.1]])
        .append_query_results(vec![vec![(asset_1.1.clone(), asset_data_1.1.clone())]])
        .into_connection();

    let insert_result = asset_data::Entity::insert(asset_data_1.0)
        .exec(&db)
        .await
        .unwrap();
    assert_eq!(insert_result.last_insert_id, 1);

    let insert_result = asset::Entity::insert(asset_1.0).exec(&db).await.unwrap();
    assert_eq!(insert_result.last_insert_id, id.to_bytes().to_vec());

    let insert_result = asset_creators::Entity::insert(asset_creator_1_1.0)
        .exec(&db)
        .await
        .unwrap();
    assert_eq!(insert_result.last_insert_id, 1);

    let insert_result = asset_authority::Entity::insert(asset_authority_1.0)
        .exec(&db)
        .await
        .unwrap();
    assert_eq!(insert_result.last_insert_id, 1);

    assert_eq!(
        asset::Entity::find_by_id(id.to_bytes().to_vec())
            .find_also_related(AssetData)
            .one(&db)
            .await?,
        Some((asset_1.1.clone(), Some(asset_data_1.1.clone())))
    );

    Ok(())
}
