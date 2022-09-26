use crate::dao::prelude::{CandyMachine, CandyMachineData};
use crate::dao::{candy_machine, candy_machine_data};
use crate::rpc::{CandyMachine as RpcCandyMachine, Creator, FreezeInfo};
use jsonpath_lib::JsonPathError;
use mime_guess::Mime;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use url::Url;

pub async fn get_candy_machine(
    db: &DatabaseConnection,
    candy_machine_id: Vec<u8>,
) -> Result<RpcCandyMachine, DbErr> {
    let (candy_machine, candy_machine_data): (candy_machine::Model, candy_machine_data::Model) =
        CandyMachine::find_by_id(candy_machine_id)
            .find_also_related(CandyMachineData)
            .one(db)
            .await
            .and_then(|o| match o {
                Some((a, Some(d))) => Ok((a, d)),
                _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
            })?;

    // TODO add builder to check if any freeze is present
    
    Ok(RpcCandyMachine {
        id: candy_machine.id,
        collection: candy_machine.collection_mint,
        freeze_info: Some(FreezeInfo {
            allow_thaw: candy_machine.allow_thaw,
            frozen_count: candy_machine.frozen_count,
            mint_start: candy_machine.mint_start,
            freeze_time: candy_machine.freeze_time,
            freeze_fee: candy_machine.freeze_fee,
        }),
        data: todo!(),
        authority: todo!(),
        wallet: todo!(),
        token_mint: todo!(),
        items_redeemed: todo!(),
    })
}
