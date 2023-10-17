use sea_orm::{DatabaseConnection, DbErr};

use crate::{
    dao::scopes,
    rpc::{display_options::DisplayOptions, Asset},
};

use super::common::asset_to_rpc;

pub async fn get_asset(
    db: &DatabaseConnection,
    id: Vec<u8>,
    display_options: &DisplayOptions,
) -> Result<Asset, DbErr> {
    let asset = scopes::asset::get_by_id(db, id, false).await?;
    asset_to_rpc(asset, display_options)
}
