use crate::dao::prelude::{CandyGuard, CandyGuardGroup, CandyMachineData};
use crate::dao::{
    candy_guard, candy_guard_group, candy_machine, candy_machine_creators, candy_machine_data,
};
use crate::rpc::filter::CandyMachineSorting;
use crate::rpc::response::CandyMachineList;
use crate::rpc::{
    CandyGuard as RpcCandyGuard, CandyGuardData, CandyGuardGroup as RpcCandyGuardGroup,
    CandyMachine as RpcCandyMachine, CandyMachineData as RpcCandyMachineData,
};
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

use super::candy_machine::{to_creators, transform_optional_pubkeys};
use super::candy_machine_helpers::{
    get_candy_guard_group, get_candy_machine_data, get_freeze_info,
};

pub async fn get_candy_machines_by_size(
    db: &DatabaseConnection,
    size: u64,
    sort_by: CandyMachineSorting,
    limit: u32,
    page: u32,
    before: Vec<u8>,
    after: Vec<u8>,
) -> Result<CandyMachineList, DbErr> {
    let sort_column = match sort_by {
        CandyMachineSorting::Created => candy_machine::Column::CreatedAt,
        CandyMachineSorting::LastMinted => candy_machine::Column::LastMinted,
    };

    let candy_machines = if page > 0 {
        let paginator = candy_machine::Entity::find()
            .join(
                JoinType::LeftJoin,
                candy_machine::Entity::has_many(candy_machine_creators::Entity).into(),
            )
            .filter(Condition::any().add(candy_machine_data::Column::MaxSupply.eq(size)))
            .find_also_related(CandyMachineData)
            .order_by_asc(sort_column)
            .paginate(db, limit.try_into().unwrap());

        paginator.fetch_page((page - 1).try_into().unwrap()).await?
    } else if !before.is_empty() {
        let rows = candy_machine::Entity::find()
            .order_by_asc(sort_column)
            .join(
                JoinType::LeftJoin,
                candy_machine::Entity::has_many(candy_machine_creators::Entity).into(),
            )
            .filter(Condition::any().add(candy_machine_data::Column::MaxSupply.eq(size)))
            .cursor_by(candy_machine_creators::Column::CandyMachineId)
            .before(before.clone())
            .first(limit.into())
            .all(db)
            .await?
            .into_iter()
            .map(|x| async move {
                let candy_machine_data = x.find_related(CandyMachineData).one(db).await.unwrap();

                (x, candy_machine_data)
            });

        let candy_machines = futures::future::join_all(rows).await;
        candy_machines
    } else {
        let rows = candy_machine::Entity::find()
            .order_by_asc(sort_column)
            .join(
                JoinType::LeftJoin,
                candy_machine::Entity::has_many(candy_machine_creators::Entity).into(),
            )
            .filter(Condition::any().add(candy_machine_data::Column::MaxSupply.eq(size)))
            .cursor_by(candy_machine_creators::Column::CandyMachineId)
            .after(after.clone())
            .first(limit.into())
            .all(db)
            .await?
            .into_iter()
            .map(|x| async move {
                let candy_machine_data = x.find_related(CandyMachineData).one(db).await.unwrap();

                (x, candy_machine_data)
            });

        let candy_machines = futures::future::join_all(rows).await;
        candy_machines
    };

    let filter_candy_machines: Result<Vec<_>, _> = candy_machines
        .into_iter()
        .map(
            |(candy_machine, candy_machine_data)| match candy_machine_data {
                Some(candy_machine_data) => Ok((candy_machine, candy_machine_data)),
                _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
            },
        )
        .collect();

    let build_candy_machine_list =
        filter_candy_machines?
            .into_iter()
            .map(|(candy_machine, candy_machine_data)| async move {
                let creators: Vec<candy_machine_creators::Model> =
                    candy_machine_creators::Entity::find()
                        .filter(
                            candy_machine_creators::Column::CandyMachineId
                                .eq(candy_machine.id.clone()),
                        )
                        .all(db)
                        .await
                        .unwrap();

                let candy_guard = if let Some(candy_guard_pda) = candy_machine.candy_guard_pda {
                    let (candy_guard, candy_guard_group): (
                        candy_guard::Model,
                        Vec<candy_guard_group::Model>,
                    ) = CandyGuard::find_by_id(candy_guard_pda)
                        .find_with_related(CandyGuardGroup)
                        .all(db)
                        .await
                        .and_then(|o| {
                            if o.len() > 0 {
                                let index = o.get(0).unwrap();
                                Ok(index.clone())
                            } else {
                                Err(DbErr::RecordNotFound("Candy Guard Not Found".to_string()))
                            }
                        })
                        .unwrap();

                    let default_set = candy_guard_group
                        .clone()
                        .into_iter()
                        .find(|group| group.label.is_none())
                        .map(|group| {
                            let guard_set = get_candy_guard_group(&group);

                            guard_set
                        })
                        .unwrap();

                    let mut groups = Vec::new();
                    for group in candy_guard_group
                        .iter()
                        .filter(|group| group.label.is_some())
                    {
                        let guard_set = get_candy_guard_group(group);

                        groups.push(RpcCandyGuardGroup {
                            label: group.label.clone().unwrap(),
                            guards: guard_set,
                        })
                    }

                    let is_groups = if groups.len() > 0 { Some(groups) } else { None };

                    Some(RpcCandyGuard {
                        id: bs58::encode(candy_guard.id).into_string(),
                        bump: candy_guard.bump,
                        authority: bs58::encode(candy_guard.authority).into_string(),
                        candy_guard_data: CandyGuardData {
                            default: default_set,
                            groups: is_groups,
                        },
                    })
                } else {
                    None
                };

                let rpc_creators = to_creators(creators);

                let freeze_info = get_freeze_info(
                    candy_machine.allow_thaw,
                    candy_machine.frozen_count,
                    candy_machine.freeze_time,
                    candy_machine.freeze_fee,
                    candy_machine.mint_start,
                );

                let (
                    data_whitelist_mint_settings,
                    data_hidden_settings,
                    data_config_line_settings,
                    data_end_settings,
                    data_gatekeeper,
                ) = get_candy_machine_data(candy_machine_data.clone());

                RpcCandyMachine {
                    id: bs58::encode(candy_machine.id).into_string(),
                    collection: transform_optional_pubkeys(candy_machine.collection_mint),
                    freeze_info,
                    data: RpcCandyMachineData {
                        uuid: candy_machine_data.uuid,
                        price: candy_machine_data.price,
                        symbol: candy_machine_data.symbol,
                        seller_fee_basis_points: candy_machine_data.seller_fee_basis_points,
                        max_supply: candy_machine_data.max_supply,
                        is_mutable: candy_machine_data.is_mutable,
                        retain_authority: candy_machine_data.retain_authority,
                        go_live_date: candy_machine_data.go_live_date,
                        items_available: candy_machine_data.items_available,
                        config_line_settings: data_config_line_settings,
                        hidden_settings: data_hidden_settings,
                        end_settings: data_end_settings,
                        gatekeeper: data_gatekeeper,
                        whitelist_mint_settings: data_whitelist_mint_settings,
                        creators: Some(rpc_creators),
                    },
                    authority: bs58::encode(candy_machine.authority).into_string(),
                    wallet: transform_optional_pubkeys(candy_machine.wallet),
                    token_mint: transform_optional_pubkeys(candy_machine.token_mint),
                    items_redeemed: candy_machine.items_redeemed,
                    features: candy_machine.features,
                    mint_authority: transform_optional_pubkeys(candy_machine.mint_authority),
                    candy_guard,
                }
            });

    let built_candy_machines = futures::future::join_all(build_candy_machine_list).await;

    let total = built_candy_machines.len() as u32;

    let page = if page > 0 { Some(page) } else { None };
    let before = if !before.is_empty() {
        Some(String::from_utf8(before).unwrap())
    } else {
        None
    };
    let after = if !after.is_empty() {
        Some(String::from_utf8(after).unwrap())
    } else {
        None
    };

    Ok(CandyMachineList {
        total,
        limit,
        page,
        before,
        after,
        items: built_candy_machines,
    })
}
