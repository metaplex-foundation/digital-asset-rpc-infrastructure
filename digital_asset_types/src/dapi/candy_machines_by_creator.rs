use crate::dao::prelude::{CandyGuard, CandyGuardGroup, CandyMachineData};
use crate::dao::{candy_guard, candy_guard_group, candy_machine, candy_machine_creators};
use crate::rpc::filter::CandyMachineSorting;
use crate::rpc::response::CandyMachineList;
use crate::rpc::{
    CandyGuard as RpcCandyGuard, CandyGuardData, CandyGuardGroup as RpcCandyGuardGroup,
    CandyMachine as RpcCandyMachine, CandyMachineData as RpcCandyMachineData, GuardSet,
};
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

use super::candy_machine::{to_creators, transform_optional_pubkeys};
use super::candy_machine_helpers::{
    get_allow_list, get_bot_tax, get_config_line_settings, get_end_settings, get_freeze_info,
    get_gatekeeper, get_hidden_settings, get_lamports, get_live_date, get_mint_limit,
    get_nft_payment, get_spl_token, get_third_party_signer, get_whitelist_settings,
};

pub async fn get_candy_machines_by_creator(
    db: &DatabaseConnection,
    creator_expression: Vec<Vec<u8>>,
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

    let mut conditions = Condition::any();
    for creator in creator_expression {
        conditions = conditions.add(candy_machine_creators::Column::Creator.eq(creator.clone()));
    }

    let candy_machines = if page > 0 {
        let paginator = candy_machine::Entity::find()
            .join(
                JoinType::LeftJoin,
                candy_machine::Entity::has_many(candy_machine_creators::Entity).into(),
            )
            .filter(conditions)
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
            .filter(conditions)
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
            .filter(conditions)
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
                _ => Err(DbErr::RecordNotFound("Candy Machine Not Found".to_string())),
            },
        )
        .collect();

    let build_candy_machine_list =
        filter_candy_machines?
            .into_iter()
            .map(|(candy_machine, candy_machine_data)| async move {
                let creators = candy_machine_creators::Entity::find()
                    .filter(
                        candy_machine_creators::Column::CandyMachineId.eq(candy_machine.id.clone()),
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
                        .and_then(|o| match o {
                            o => {
                                let index = o.get(0).unwrap();
                                Ok(index.clone())
                            }
                            _ => Err(DbErr::RecordNotFound("Candy Guard Not Found".to_string())),
                        })
                        .unwrap();

                    let default_set = candy_guard_group
                        .clone()
                        .into_iter()
                        .find(|group| group.label.is_none())
                        .map(|group| {
                            let gatekeeper = get_gatekeeper(
                                group.gatekeeper_network,
                                group.gatekeeper_expire_on_use,
                            );
                            let lamports =
                                get_lamports(group.lamports_amount, group.lamports_destination);
                            let spl_token = get_spl_token(
                                group.spl_token_amount,
                                group.spl_token_mint,
                                group.spl_token_destination_ata,
                            );
                            let third_party_signer =
                                get_third_party_signer(group.third_party_signer_key);
                            let allow_list = get_allow_list(group.allow_list_merkle_root);
                            let nft_payment = get_nft_payment(
                                group.nft_payment_burn,
                                group.nft_payment_required_collection,
                            );
                            let whitelist_settings = get_whitelist_settings(
                                group.whitelist_mode,
                                group.whitelist_mint,
                                group.whitelist_presale,
                                group.whitelist_discount_price,
                            );

                            let end_settings =
                                get_end_settings(group.end_setting_number, group.end_setting_type);

                            let mint_limit =
                                get_mint_limit(group.mint_limit_id, group.mint_limit_limit);

                            let live_date = get_live_date(group.live_date);
                            let bot_tax =
                                get_bot_tax(group.bot_tax_lamports, group.bot_tax_last_instruction);

                            GuardSet {
                                bot_tax,
                                lamports,
                                spl_token,
                                live_date,
                                third_party_signer,
                                whitelist: whitelist_settings,
                                gatekeeper,
                                end_settings,
                                allow_list,
                                mint_limit,
                                nft_payment,
                            }
                        })
                        .unwrap();

                    let mut groups = Vec::new();
                    for group in candy_guard_group
                        .iter()
                        .filter(|group| group.label.is_some())
                    {
                        let gatekeeper = get_gatekeeper(
                            group.gatekeeper_network.clone(),
                            group.gatekeeper_expire_on_use,
                        );
                        let lamports =
                            get_lamports(group.lamports_amount, group.lamports_destination.clone());
                        let spl_token = get_spl_token(
                            group.spl_token_amount,
                            group.spl_token_mint.clone(),
                            group.spl_token_destination_ata.clone(),
                        );
                        let third_party_signer =
                            get_third_party_signer(group.third_party_signer_key.clone());
                        let allow_list = get_allow_list(group.allow_list_merkle_root.clone());
                        let nft_payment = get_nft_payment(
                            group.nft_payment_burn,
                            group.nft_payment_required_collection.clone(),
                        );
                        let whitelist_settings = get_whitelist_settings(
                            group.whitelist_mode.clone(),
                            group.whitelist_mint.clone(),
                            group.whitelist_presale,
                            group.whitelist_discount_price,
                        );

                        let end_settings = get_end_settings(
                            group.end_setting_number,
                            group.end_setting_type.clone(),
                        );

                        let mint_limit =
                            get_mint_limit(group.mint_limit_id, group.mint_limit_limit);
                        let live_date = get_live_date(group.live_date);
                        let bot_tax =
                            get_bot_tax(group.bot_tax_lamports, group.bot_tax_last_instruction);

                        groups.push(RpcCandyGuardGroup {
                            label: group.label.clone().unwrap(),
                            guards: GuardSet {
                                bot_tax,
                                lamports,
                                spl_token,
                                live_date,
                                third_party_signer,
                                whitelist: whitelist_settings,
                                gatekeeper,
                                end_settings,
                                allow_list,
                                mint_limit,
                                nft_payment,
                            },
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

                let data_hidden_settings = get_hidden_settings(
                    candy_machine_data.hidden_settings_name,
                    candy_machine_data.hidden_settings_uri,
                    candy_machine_data.hidden_settings_hash,
                );
                let data_gatekeeper = get_gatekeeper(
                    candy_machine_data.gatekeeper_network,
                    candy_machine_data.gatekeeper_expire_on_use,
                );
                let data_whitelist_mint_settings = get_whitelist_settings(
                    candy_machine_data.whitelist_mode,
                    candy_machine_data.whitelist_mint,
                    candy_machine_data.whitelist_presale,
                    candy_machine_data.whitelist_discount_price,
                );

                let data_end_settings = get_end_settings(
                    candy_machine_data.end_setting_number,
                    candy_machine_data.end_setting_type,
                );

                let data_config_line_settings = get_config_line_settings(
                    candy_machine_data.config_line_settings_is_sequential,
                    candy_machine_data.config_line_settings_name_length,
                    candy_machine_data.config_line_settings_prefix_name,
                    candy_machine_data.config_line_settings_prefix_uri,
                    candy_machine_data.config_line_settings_uri_length,
                );

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
