use super::common::*;
use das_api::api::ApiContract;
use digital_asset_types::dao::slot_metas;
use function_name::named;
use sea_orm::{ActiveValue::Set, EntityTrait};
use serial_test::serial;

#[tokio::test]
#[serial]
#[named]
async fn test_get_slot() {
    let name = trim_test_name(function_name!());
    let setup = TestSetup::new(name.clone()).await;

    apply_migrations_and_delete_data(setup.db.clone()).await;
    let model = slot_metas::ActiveModel { slot: Set(12345) };
    slot_metas::Entity::insert(model)
        .exec(&setup.das_api.get_connection())
        .await
        .unwrap();

    let response = setup.das_api.get_slot(None).await.unwrap();

    insta::assert_json_snapshot!(name, response);
}
