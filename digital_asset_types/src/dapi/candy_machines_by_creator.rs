use crate::dao::candy_machine;
use crate::dao::prelude::CandyMachineData;
use crate::rpc::filter::CandyMachineSorting;
use crate::rpc::response::CandyMachineList;
use crate::rpc::CandyMachine as RpcCandyMachine;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

pub async fn get_candy_machines_by_creator(
    db: &DatabaseConnection,
    creator_expression: Vec<Vec<u8>>,
    sort_by: CandyMachineSorting,
    limit: u32,
    page: u32,
    before: Vec<u8>,
    after: Vec<u8>,
) -> Result<CandyMachineList, DbErr> {
}
