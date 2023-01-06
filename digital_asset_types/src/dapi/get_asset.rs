use sea_orm::{DatabaseConnection, DbErr};

use crate::{dao::scopes, rpc::Asset};

use super::common::asset_to_rpc;

pub async fn get_asset(db: &DatabaseConnection, id: Vec<u8>) -> Result<Asset, DbErr> {
    let asset = scopes::asset::get_by_id(db, id).await?;
    asset_to_rpc(asset)
}
