use crate::common::{index_seed_events, trim_test_name, Network, TestSetupOptions};
use digital_asset_types::dao::{asset, token_accounts, tokens};
use function_name::named;
use sea_orm::{ActiveValue, EntityTrait};
use serial_test::serial;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use super::common::{apply_migrations_and_delete_data, seed_txn, TestSetup};

#[tokio::test]
#[serial]
#[named]
async fn test_ta_account_close() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;
    let ta_bytes = Pubkey::from_str("8Xv3SpX94HHf32Apg4TeSeS3i2p6wuXeE8FBZr168Hti")
        .unwrap()
        .to_bytes()
        .to_vec();

    apply_migrations_and_delete_data(setup.db.clone()).await;

    // seed account to db
    let ta_model = token_accounts::ActiveModel {
        pubkey: ActiveValue::Set(ta_bytes.clone()),
        mint: ActiveValue::Set(Pubkey::new_unique().to_bytes().to_vec()),
        owner: ActiveValue::Set(Pubkey::new_unique().to_bytes().to_vec()),
        token_program: ActiveValue::Set(Pubkey::new_unique().to_bytes().to_vec()),
        slot_updated: ActiveValue::Set(0),
        ..Default::default()
    };

    token_accounts::Entity::insert(ta_model)
        .exec(setup.db.as_ref())
        .await
        .unwrap();

    let seeds= [
        // Txn with close token_account ix
        seed_txn("2y6wfUvHZt5xsX5wv4P49LfiBNtqRqTZioM2RCYTiW4WWY17zEFMA3Bc2DLNPEFFvEbSuH7t5fqL73uBiwrPFj53")
    ];

    index_seed_events(&setup, seeds.iter().collect()).await;

    let res = token_accounts::Entity::find_by_id(ta_bytes)
        .one(setup.db.as_ref())
        .await
        .unwrap();

    assert!(res.is_none());
}

#[tokio::test]
#[serial]
#[named]
async fn test_mint_account_close() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new_with_options(
        name,
        TestSetupOptions {
            network: Some(Network::Devnet),
        },
    )
    .await;

    let m_bytes = Pubkey::from_str("4CdYhHZiSzvPXtc3PvgFLES4tFxAP5yGSbgFXp1pfGUZ")
        .unwrap()
        .to_bytes()
        .to_vec();

    apply_migrations_and_delete_data(setup.db.clone()).await;

    // seed account to db
    let m_model = tokens::ActiveModel {
        mint: ActiveValue::Set(m_bytes.clone()),
        slot_updated: ActiveValue::Set(0),
        token_program: ActiveValue::Set(Pubkey::new_unique().to_bytes().to_vec()),
        ..Default::default()
    };

    let asset_model = asset::ActiveModel {
        id: ActiveValue::Set(m_bytes.clone()),
        burnt: ActiveValue::Set(false),
        ..Default::default()
    };

    let (token_result, asset_result) = tokio::join!(
        tokens::Entity::insert(m_model).exec(setup.db.as_ref()),
        asset::Entity::insert(asset_model).exec(setup.db.as_ref())
    );

    token_result.unwrap();
    asset_result.unwrap();

    let seeds= [
        // Txn with close mint ix 
        seed_txn("3zcW7h9nXZcf8xTPm22uejGCeZzUhxUxjVuph2zxbMdDLFC6WWjv2VZT1pvKJ3Ho1JuuAk24WF9gGPaVxLFsfq25")
    ];

    index_seed_events(&setup, seeds.iter().collect()).await;

    let (mint_res, asset_res) = tokio::join!(
        tokens::Entity::find_by_id(m_bytes.clone()).one(setup.db.as_ref()),
        asset::Entity::find_by_id(m_bytes).one(setup.db.as_ref())
    );

    assert!(mint_res.unwrap().is_none());
    assert!(asset_res.unwrap().unwrap().burnt);
}
