use crate::dao::prelude::{CandyMachine, CandyMachineData};
use crate::dao::{candy_machine, candy_machine_data};
use crate::rpc::{
    Asset as RpcAsset, Authority, Compression, Content, Creator, File, Group, Interface, Ownership,
    Royalty, Scope,
};
use jsonpath_lib::JsonPathError;
use mime_guess::Mime;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use url::Url;

pub async fn get_asset(
    db: &DatabaseConnection,
    candy_machine_id: Vec<u8>,
) -> Result<RpcAsset, DbErr> {
    let candy_machine_data: (candy_machine::Model, candy_machine_data::Model) =
        CandyMachine::find_by_id(candy_machine_id)
            .find_also_related(CandyMachineData)
            .one(db)
            .await
            .and_then(|o| match o {
                Some((a, Some(d))) => Ok((a, d)),
                _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
            })?;
}
