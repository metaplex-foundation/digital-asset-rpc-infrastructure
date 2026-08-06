#[cfg(test)]
mod common;

use blockbuster::token_metadata::types::{Creator, TokenStandard};
use common::*;
use digital_asset_types::dao::scopes::asset::get_by_id;
use digital_asset_types::dao::sea_orm_active_enums::*;
use digital_asset_types::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping,
    sea_orm_active_enums::{OwnerType, RoyaltyTargetType},
};
use digital_asset_types::dao::{FullAsset, SELLER_FEE_BASIS_POINTS_INHERIT};
use digital_asset_types::dapi::common::asset_to_rpc;
use digital_asset_types::rpc::options::Options;
use sea_orm::{entity::prelude::*, DatabaseBackend, MockDatabase};
use solana_sdk::{signature::Keypair, signer::Signer};

#[tokio::test]
async fn get_asset_by_id() -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let update_authority = Keypair::new().pubkey();
    let creator_1 = Keypair::new().pubkey();
    let uri = Keypair::new().pubkey();

    let metadata_1 = MockMetadataArgs {
        name: String::from("Test #1"),
        symbol: String::from("BUBBLE"),
        uri: uri.to_string(),
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
        }],
        seller_fee_basis_points: 100,
    };

    let asset_data_1 = create_asset_data(metadata_1.clone(), id.to_bytes().to_vec());
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
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        metadata_1.seller_fee_basis_points as i32,
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

    let _insert_result = asset_data::Entity::insert(asset_data_1.0)
        .exec(&db)
        .await
        .unwrap();

    let insert_result = asset::Entity::insert(asset_1.0).exec(&db).await.unwrap();
    assert_eq!(insert_result.last_insert_id, id.to_bytes().to_vec());

    let _insert_result = asset_creators::Entity::insert(asset_creator_1_1.0)
        .exec(&db)
        .await
        .unwrap();

    let _insert_result = asset_authority::Entity::insert(asset_authority_1.0)
        .exec(&db)
        .await
        .unwrap();

    assert_eq!(
        asset::Entity::find_by_id(id.to_bytes().to_vec())
            .find_also_related(asset_data::Entity)
            .one(&db)
            .await?,
        Some((asset_1.1.clone(), Some(asset_data_1.1.clone())))
    );

    Ok(())
}

#[tokio::test]
async fn asset_to_rpc_resolves_inherited_bubblegum_v2_royalties() -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let collection = Keypair::new().pubkey();
    let uri = Keypair::new().pubkey();

    let metadata = MockMetadataArgs {
        name: String::from("Inherited Royalty NFT"),
        symbol: String::from("BUBBLE"),
        uri: uri.to_string(),
        primary_sale_happened: false,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![],
        seller_fee_basis_points: SELLER_FEE_BASIS_POINTS_INHERIT as u16,
    };

    let asset_data = create_asset_data(metadata, id.to_bytes().to_vec());
    let mut asset = create_asset(
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
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        SELLER_FEE_BASIS_POINTS_INHERIT,
    )
    .1;
    asset.specification_asset_class = Some(SpecificationAssetClass::MplBubblegumV2);
    asset.collection_hash = Some(collection.to_string());

    let grouping = create_asset_grouping(id.to_bytes().to_vec(), collection, 1).1;
    let rpc_asset = asset_to_rpc(
        FullAsset {
            asset,
            data: asset_data.1,
            token_info: None,
            authorities: vec![],
            creators: vec![],
            inscription: None,
            groups: vec![(grouping, None)],
            inherited_collection_royalty: Some(500),
            inherited_collection_creators: None,
        },
        &Options::default(),
    )?;

    let royalty = rpc_asset.royalty.expect("royalty should be present");
    assert_eq!(royalty.basis_points, 500);
    assert_eq!(
        royalty.basis_points_raw,
        Some(SELLER_FEE_BASIS_POINTS_INHERIT as u32)
    );
    assert_eq!(royalty.sfbp_inherited, Some(true));
    assert!((royalty.percent - 0.05).abs() < f64::EPSILON);
    assert_eq!(rpc_asset.creators_raw, Some(vec![]));

    Ok(())
}

#[tokio::test]
async fn asset_to_rpc_bubblegum_v2_includes_cnft_creators_as_destination() -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let creator = Keypair::new().pubkey();
    let uri = Keypair::new().pubkey();

    let metadata = MockMetadataArgs {
        name: String::from("Creator Royalty NFT"),
        symbol: String::from("BUBBLE"),
        uri: uri.to_string(),
        primary_sale_happened: false,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![Creator {
            address: creator,
            share: 100,
            verified: false,
        }],
        seller_fee_basis_points: 500,
    };

    let asset_data = create_asset_data(metadata, id.to_bytes().to_vec());
    let mut asset = create_asset(
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
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        500,
    )
    .1;
    asset.specification_asset_class = Some(SpecificationAssetClass::MplBubblegumV2);

    let creator_row = create_asset_creator(
        id.to_bytes().to_vec(),
        creator.to_bytes().to_vec(),
        100,
        false,
        1,
    )
    .1;

    let rpc_asset = asset_to_rpc(
        FullAsset {
            asset,
            data: asset_data.1,
            token_info: None,
            authorities: vec![],
            creators: vec![creator_row],
            inscription: None,
            groups: vec![],
            inherited_collection_royalty: None,
            inherited_collection_creators: None,
        },
        &Options::default(),
    )?;

    let royalty = rpc_asset.royalty.expect("royalty should be present");
    assert_eq!(
        royalty.royalty_model,
        digital_asset_types::rpc::RoyaltyModel::Creators
    );
    assert_eq!(royalty.target, None);
    assert_eq!(royalty.basis_points, 500);

    let creators = rpc_asset.creators.expect("creators should be present");
    assert_eq!(creators.len(), 1);
    assert_eq!(creators[0].address, creator.to_string());
    assert_eq!(creators[0].share, 100);
    assert_eq!(rpc_asset.creators_raw, None);

    Ok(())
}

#[tokio::test]
async fn asset_to_rpc_inherited_sfbp_uses_collection_creators_when_cnft_has_none(
) -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let collection = Keypair::new().pubkey();
    let collection_creator = Keypair::new().pubkey();

    let (asset, data, grouping) = inherited_bubblegum_v2_asset(id, owner, collection);
    let collection_creator_row = create_asset_creator(
        collection.to_bytes().to_vec(),
        collection_creator.to_bytes().to_vec(),
        100,
        false,
        1,
    )
    .1;

    let rpc_asset = asset_to_rpc(
        FullAsset {
            asset,
            data,
            token_info: None,
            authorities: vec![],
            creators: vec![],
            inscription: None,
            groups: vec![(grouping, None)],
            inherited_collection_royalty: Some(750),
            inherited_collection_creators: Some(vec![collection_creator_row]),
        },
        &Options::default(),
    )?;

    let royalty = rpc_asset.royalty.expect("royalty should be present");
    assert_eq!(royalty.basis_points, 750);
    assert_eq!(royalty.target, None);

    let creators = rpc_asset.creators.expect("creators should be present");
    assert_eq!(creators.len(), 1);
    assert_eq!(creators[0].address, collection_creator.to_string());
    assert_eq!(creators[0].share, 100);
    assert_eq!(rpc_asset.creators_raw, Some(vec![]));

    Ok(())
}

#[tokio::test]
async fn asset_to_rpc_inherited_sfbp_always_uses_collection_creators() -> Result<(), DbErr> {
    let id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let collection = Keypair::new().pubkey();
    let cnft_creator = Keypair::new().pubkey();
    let collection_creator = Keypair::new().pubkey();

    let (asset, data, grouping) = inherited_bubblegum_v2_asset(id, owner, collection);
    let cnft_creator_row = create_asset_creator(
        id.to_bytes().to_vec(),
        cnft_creator.to_bytes().to_vec(),
        100,
        false,
        1,
    )
    .1;
    let collection_creator_row = create_asset_creator(
        collection.to_bytes().to_vec(),
        collection_creator.to_bytes().to_vec(),
        100,
        false,
        1,
    )
    .1;

    let rpc_asset = asset_to_rpc(
        FullAsset {
            asset,
            data,
            token_info: None,
            authorities: vec![],
            // Stale or pre-enforcement rows may still exist in the DB; DAS ignores them.
            creators: vec![cnft_creator_row],
            inscription: None,
            groups: vec![(grouping, None)],
            inherited_collection_royalty: Some(750),
            inherited_collection_creators: Some(vec![collection_creator_row]),
        },
        &Options::default(),
    )?;

    let creators = rpc_asset.creators.expect("creators should be present");
    assert_eq!(creators.len(), 1);
    assert_eq!(creators[0].address, collection_creator.to_string());

    let creators_raw = rpc_asset
        .creators_raw
        .expect("creators_raw should be present");
    assert_eq!(creators_raw.len(), 1);
    assert_eq!(creators_raw[0].address, cnft_creator.to_string());

    Ok(())
}

fn inherited_bubblegum_v2_asset(
    id: solana_sdk::pubkey::Pubkey,
    owner: solana_sdk::pubkey::Pubkey,
    collection: solana_sdk::pubkey::Pubkey,
) -> (asset::Model, asset_data::Model, asset_grouping::Model) {
    let metadata = MockMetadataArgs {
        name: String::from("Inherited Royalty NFT"),
        symbol: String::from("BUBBLE"),
        uri: Keypair::new().pubkey().to_string(),
        primary_sale_happened: false,
        is_mutable: true,
        edition_nonce: None,
        token_standard: Some(TokenStandard::NonFungible),
        collection: None,
        uses: None,
        creators: vec![],
        seller_fee_basis_points: SELLER_FEE_BASIS_POINTS_INHERIT as u16,
    };

    let asset_data = create_asset_data(metadata, id.to_bytes().to_vec()).1;
    let mut cnft = create_asset(
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
        Some(SpecificationVersions::V1),
        Some(0_i64),
        None,
        RoyaltyTargetType::Creators,
        None,
        SELLER_FEE_BASIS_POINTS_INHERIT,
    )
    .1;
    cnft.specification_asset_class = Some(SpecificationAssetClass::MplBubblegumV2);

    let mut grouping = create_asset_grouping(id.to_bytes().to_vec(), collection, 1).1;
    grouping.verified = true;

    (cnft, asset_data, grouping)
}

#[tokio::test]
async fn get_by_id_hydrates_inherited_bubblegum_v2_royalties() -> Result<(), DbErr> {
    let asset_id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let collection = Keypair::new().pubkey();

    let (cnft, cnft_data, grouping) = inherited_bubblegum_v2_asset(asset_id, owner, collection);

    let collection_asset = create_asset(
        collection.to_bytes().to_vec(),
        collection.to_bytes().to_vec(),
        OwnerType::Single,
        None,
        false,
        1,
        None,
        false,
        false,
        None,
        Some(SpecificationVersions::V1),
        None,
        None,
        RoyaltyTargetType::Creators,
        None,
        750,
    );
    let mut collection_model = collection_asset.1;
    collection_model.specification_asset_class = Some(SpecificationAssetClass::MplCoreCollection);

    let collection_creator = Keypair::new().pubkey();
    let collection_creator_row = create_asset_creator(
        collection.to_bytes().to_vec(),
        collection_creator.to_bytes().to_vec(),
        100,
        false,
        1,
    )
    .1;

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![(cnft.clone(), cnft_data.clone())]])
        .append_query_results(vec![Vec::<asset_authority::Model>::new()])
        .append_query_results(vec![Vec::<asset_creators::Model>::new()])
        .append_query_results(vec![vec![grouping.clone()]])
        .append_query_results(vec![vec![collection_model.clone()]])
        .append_query_results(vec![vec![collection_creator_row.clone()]])
        .into_connection();

    let full_asset = get_by_id(&db, asset_id.to_bytes().to_vec(), &Options::default()).await?;

    assert_eq!(full_asset.inherited_collection_royalty, Some(750));
    assert_eq!(
        full_asset
            .inherited_collection_creators
            .as_ref()
            .map(|c| c.len()),
        Some(1)
    );

    let rpc_asset = asset_to_rpc(full_asset, &Options::default())?;
    let royalty = rpc_asset.royalty.expect("royalty should be present");
    assert_eq!(royalty.basis_points, 750);
    assert_eq!(
        royalty.basis_points_raw,
        Some(SELLER_FEE_BASIS_POINTS_INHERIT as u32)
    );
    assert_eq!(royalty.sfbp_inherited, Some(true));
    assert_eq!(royalty.target, None);

    let creators = rpc_asset.creators.expect("creators should be present");
    assert_eq!(creators.len(), 1);
    assert_eq!(creators[0].address, collection_creator.to_string());
    assert_eq!(rpc_asset.creators_raw, Some(vec![]));

    Ok(())
}

#[tokio::test]
async fn get_by_id_leaves_inherited_royalty_none_when_collection_missing() -> Result<(), DbErr> {
    let asset_id = Keypair::new().pubkey();
    let owner = Keypair::new().pubkey();
    let collection = Keypair::new().pubkey();

    let (cnft, cnft_data, grouping) = inherited_bubblegum_v2_asset(asset_id, owner, collection);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![(cnft, cnft_data)]])
        .append_query_results(vec![Vec::<asset_authority::Model>::new()])
        .append_query_results(vec![Vec::<asset_creators::Model>::new()])
        .append_query_results(vec![vec![grouping]])
        .append_query_results(vec![Vec::<asset::Model>::new()])
        .append_query_results(vec![Vec::<asset_creators::Model>::new()])
        .into_connection();

    let full_asset = get_by_id(&db, asset_id.to_bytes().to_vec(), &Options::default()).await?;

    assert_eq!(full_asset.inherited_collection_royalty, None);

    let rpc_asset = asset_to_rpc(full_asset, &Options::default())?;
    let royalty = rpc_asset.royalty.expect("royalty should be present");
    assert_eq!(royalty.basis_points, 0);
    assert_eq!(
        royalty.basis_points_raw,
        Some(SELLER_FEE_BASIS_POINTS_INHERIT as u32)
    );
    assert_eq!(royalty.sfbp_inherited, Some(true));

    Ok(())
}
