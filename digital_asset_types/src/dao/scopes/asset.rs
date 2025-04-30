use crate::{
    dao::{
        asset::{self},
        asset_authority, asset_creators, asset_data, asset_grouping, asset_v1_account_attachments,
        cl_audits_v2,
        extensions::{self, instruction::PascalCase},
        sea_orm_active_enums::{Instruction, V1AccountAttachments},
        token_accounts, tokens, Cursor, FullAsset, GroupingSize, Pagination,
    },
    rpc::{
        filter::AssetSortDirection,
        options::Options,
        response::{NftEdition, NftEditions},
        RpcTokenAccountBalance, RpcTokenSupply, SolanaRpcContext, SolanaRpcResponseAndContext,
        UiTokenAmount,
    },
};
use indexmap::IndexMap;
use mpl_token_metadata::accounts::{Edition, MasterEdition};
use num_traits::ToPrimitive;
use sea_orm::{
    entity::*, prelude::Decimal, query::*, sea_query::Expr, ConnectionTrait, DbErr, Order,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, hash::RandomState, ops::Div};

pub fn paginate<T, C>(
    pagination: &Pagination,
    limit: u64,
    stmt: T,
    sort_direction: Order,
    column: C,
) -> T
where
    T: QueryFilter + QuerySelect,
    C: ColumnTrait,
{
    let mut stmt = stmt;
    match pagination {
        Pagination::Keyset { before, after } => {
            if let Some(b) = before {
                stmt = stmt.filter(column.lt(b.clone()));
            }
            if let Some(a) = after {
                stmt = stmt.filter(column.gt(a.clone()));
            }
        }
        Pagination::Page { page } => {
            if *page > 0 {
                stmt = stmt.offset((page - 1) * limit)
            }
        }
        Pagination::Cursor(cursor) => {
            if *cursor != Cursor::default() {
                if sort_direction == sea_orm::Order::Asc {
                    stmt = stmt.filter(column.gt(cursor.id.clone()));
                } else {
                    stmt = stmt.filter(column.lt(cursor.id.clone()));
                }
            }
        }
    }
    stmt.limit(limit)
}

#[allow(clippy::too_many_arguments)]
pub async fn get_by_creator(
    conn: &impl ConnectionTrait,
    creator: Vec<u8>,
    only_verified: bool,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = Condition::all()
        .add(asset_creators::Column::Creator.eq(creator.clone()))
        .add(asset::Column::Supply.gt(0));
    if only_verified {
        condition = condition.add(asset_creators::Column::Verified.eq(true));
    }
    get_by_related_condition(
        conn,
        condition,
        extensions::asset::Relation::AssetCreators,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
        Some(creator),
    )
    .await
}

pub async fn get_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
) -> Result<GroupingSize, DbErr> {
    let size = asset_grouping::Entity::find()
        .filter(
            Condition::all()
                .add(asset_grouping::Column::GroupKey.eq(group_key))
                .add(asset_grouping::Column::GroupValue.eq(group_value))
                .add(
                    Condition::any()
                        .add(asset_grouping::Column::Verified.eq(true))
                        .add(asset_grouping::Column::Verified.is_null()),
                ),
        )
        .count(conn)
        .await?;
    Ok(GroupingSize { size })
}

#[allow(clippy::too_many_arguments)]
pub async fn get_by_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = asset_grouping::Column::GroupKey
        .eq(group_key)
        .and(asset_grouping::Column::GroupValue.eq(group_value));

    if !options.show_unverified_collections {
        condition = condition.and(
            asset_grouping::Column::Verified
                .eq(true)
                .or(asset_grouping::Column::Verified.is_null()),
        );
    }

    get_by_related_condition(
        conn,
        Condition::all()
            .add(condition)
            .add(asset::Column::Supply.gt(0)),
        extensions::asset::Relation::AssetGrouping,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
        None,
    )
    .await
}

pub async fn get_assets_by_owner(
    conn: &impl ConnectionTrait,
    owner: Vec<u8>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let (join, condition) = if options.show_fungible {
        let rel = extensions::token_accounts::Relation::Asset
            .def()
            .rev()
            .on_condition(move |left, right| {
                Condition::all().add(
                    Expr::tbl(right, token_accounts::Column::Mint)
                        .eq(Expr::tbl(left, asset::Column::Id)),
                )
            });
        (
            Some(rel),
            Condition::all().add(asset::Column::Supply.gt(0)).add(
                Condition::any()
                    .add(asset::Column::Owner.eq(owner.clone()))
                    .add(token_accounts::Column::Owner.eq(owner.clone())),
            ),
        )
    } else {
        (
            None,
            Condition::all()
                .add(asset::Column::Supply.gt(0))
                .add(asset::Column::Owner.eq(owner.clone())),
        )
    };

    let joins = join.map(|j| vec![j]).unwrap_or_default();
    get_assets_by_condition(
        conn,
        condition,
        joins,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
    )
    .await
}

pub async fn get_assets(
    conn: &impl ConnectionTrait,
    asset_ids: Vec<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let cond = Condition::all()
        .add(asset::Column::Id.is_in(asset_ids))
        .add(asset::Column::Supply.gt(0));

    get_assets_by_condition(
        conn,
        cond,
        vec![],
        // Default values provided. The args below are not used for batch requests
        None,
        Order::Asc,
        pagination,
        limit,
        options,
    )
    .await
}

pub async fn get_by_authority(
    conn: &impl ConnectionTrait,
    authority: Vec<u8>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let cond = Condition::all()
        .add(asset_authority::Column::Authority.eq(authority))
        .add(asset::Column::Supply.gt(0));
    get_by_related_condition(
        conn,
        cond,
        extensions::asset::Relation::AssetAuthority,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn get_by_related_condition<E>(
    conn: &impl ConnectionTrait,
    condition: Condition,
    relation: E,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
    required_creator: Option<Vec<u8>>,
) -> Result<Vec<FullAsset>, DbErr>
where
    E: RelationTrait,
{
    let mut stmt = asset::Entity::find()
        .filter(condition)
        .join(JoinType::LeftJoin, relation.def());

    if let Some(col) = sort_by {
        stmt = stmt
            .order_by(col, sort_direction.clone())
            .order_by(asset::Column::Id, sort_direction.clone());
    }

    let assets = paginate(pagination, limit, stmt, sort_direction, asset::Column::Id)
        .all(conn)
        .await?;
    get_related_for_assets(conn, assets, options, required_creator).await
}

pub async fn get_related_for_assets(
    conn: &impl ConnectionTrait,
    assets: Vec<asset::Model>,
    options: &Options,
    required_creator: Option<Vec<u8>>,
) -> Result<Vec<FullAsset>, DbErr> {
    let asset_ids = assets.iter().map(|a| a.id.clone()).collect::<Vec<_>>();

    let asset_data: Vec<asset_data::Model> = asset_data::Entity::find()
        .filter(asset_data::Column::Id.is_in(asset_ids.clone()))
        .all(conn)
        .await?;
    let asset_data_map = asset_data.into_iter().fold(HashMap::new(), |mut acc, ad| {
        acc.insert(ad.id.clone(), ad);
        acc
    });

    // Using IndexMap to preserve order.
    let mut assets_map = assets.into_iter().fold(IndexMap::new(), |mut acc, asset| {
        if let Some(ad) = asset
            .asset_data
            .clone()
            .and_then(|ad_id| asset_data_map.get(&ad_id))
        {
            let id = asset.id.clone();
            let fa = FullAsset {
                asset,
                data: ad.clone(),
                authorities: vec![],
                creators: vec![],
                groups: vec![],
                inscription: None,
                token_info: None,
            };
            acc.insert(id, fa);
        };
        acc
    });
    let ids = assets_map.keys().cloned().collect::<Vec<_>>();

    // Get all creators for all assets in `assets_map``.
    let creators = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.is_in(ids))
        .order_by_asc(asset_creators::Column::AssetId)
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;

    // Add the creators to the assets in `asset_map``.
    for c in creators.into_iter() {
        if let Some(asset) = assets_map.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    // Filter out stale creators from each asset.
    for (_id, asset) in assets_map.iter_mut() {
        filter_out_stale_creators(&mut asset.creators);
    }

    // If we passed in a required creator, we make sure that creator is still in the creator array
    // of each asset after stale creators were filtered out above.  Only retain those assets that
    // have the required creator.  This corrects `getAssetByCreators` from returning assets for
    // which the required creator is no longer in the creator array.
    if let Some(required) = required_creator {
        assets_map.retain(|_id, asset| asset.creators.iter().any(|c| c.creator == required));
    }

    let ids = assets_map.keys().cloned().collect::<Vec<_>>();
    let authorities = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.is_in(ids.clone()))
        .order_by_asc(asset_authority::Column::AssetId)
        .all(conn)
        .await?;
    for a in authorities.into_iter() {
        if let Some(asset) = assets_map.get_mut(&a.asset_id) {
            asset.authorities.push(a);
        }
    }

    if options.show_fungible {
        find_tokens(conn, ids.clone())
            .await?
            .into_iter()
            .for_each(|t| {
                if let Some(asset) = assets_map.get_mut(&t.mint.clone()) {
                    asset.token_info = Some(t);
                }
            });
    }

    let cond = if options.show_unverified_collections {
        Condition::all()
    } else {
        Condition::any()
            .add(asset_grouping::Column::Verified.eq(true))
            // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
            // Therefore if verified is null, we can assume that the group is verified.
            .add(asset_grouping::Column::Verified.is_null())
    };

    let grouping_base_query = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.is_in(ids.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(cond)
        .order_by_asc(asset_grouping::Column::AssetId);

    if options.show_inscription {
        let attachments = asset_v1_account_attachments::Entity::find()
            .filter(asset_v1_account_attachments::Column::AssetId.is_in(asset_ids))
            .all(conn)
            .await?;

        for a in attachments.into_iter() {
            if let Some(asset) = assets_map.get_mut(&a.id) {
                asset.inscription = Some(a);
            }
        }
    }

    if options.show_collection_metadata {
        let groups = grouping_base_query.all(conn).await?;

        let group_values = groups
            .iter()
            .filter_map(|group| {
                group
                    .group_value
                    .as_ref()
                    .and_then(|g| bs58::decode(g).into_vec().ok())
            })
            .collect::<Vec<_>>();

        let asset_data = asset_data::Entity::find()
            .filter(asset_data::Column::Id.is_in(group_values))
            .order_by_asc(asset_data::Column::Id)
            .all(conn)
            .await?;

        let asset_data_map: HashMap<_, _, RandomState> = HashMap::from_iter(
            asset_data
                .into_iter()
                .map(|ad| (ad.id.clone(), ad))
                .collect::<Vec<_>>(),
        );

        for g in groups.into_iter() {
            if let Some(asset) = assets_map.get_mut(&g.asset_id) {
                let a = g.group_value.as_ref().and_then(|g| {
                    bs58::decode(g)
                        .into_vec()
                        .ok()
                        .and_then(|v| asset_data_map.get(&v))
                        .cloned()
                });

                asset.groups.push((g, a));
            }
        }
    } else {
        let single_group_query = grouping_base_query.all(conn).await?;
        for g in single_group_query.into_iter() {
            if let Some(asset) = assets_map.get_mut(&g.asset_id) {
                asset.groups.push((g, None));
            }
        }
    };

    Ok(assets_map.into_iter().map(|(_, v)| v).collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn get_assets_by_condition(
    conn: &impl ConnectionTrait,
    condition: Condition,
    joins: Vec<RelationDef>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut stmt = asset::Entity::find();
    for def in joins {
        stmt = stmt.join(JoinType::LeftJoin, def);
    }
    stmt = stmt.filter(condition);
    if let Some(col) = sort_by {
        stmt = stmt
            .order_by(col, sort_direction.clone())
            .order_by(asset::Column::Id, sort_direction.clone());
    }

    let assets = paginate(pagination, limit, stmt, sort_direction, asset::Column::Id)
        .all(conn)
        .await?;

    let full_assets = get_related_for_assets(conn, assets, options, None).await?;
    Ok(full_assets)
}

pub async fn get_by_id(
    conn: &impl ConnectionTrait,
    asset_id: Vec<u8>,
    options: &Options,
) -> Result<FullAsset, DbErr> {
    let asset_data =
        asset::Entity::find_by_id(asset_id.clone()).find_also_related(asset_data::Entity);

    let inscription = if options.show_inscription {
        get_inscription_by_mint(conn, asset_id.clone()).await.ok()
    } else {
        None
    };

    let token_info = if options.show_fungible {
        find_tokens(conn, vec![asset_id.clone()])
            .await?
            .into_iter()
            .next()
    } else {
        None
    };

    let (asset, data): (asset::Model, asset_data::Model) =
        asset_data.one(conn).await.and_then(|o| match o {
            Some((a, Some(d))) => Ok((a, d)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })?;

    if asset.supply == Decimal::from(0) {
        return Err(DbErr::Custom("Asset has no supply".to_string()));
    }

    let authorities: Vec<asset_authority::Model> = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_authority::Column::AssetId)
        .all(conn)
        .await?;

    let mut creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;

    filter_out_stale_creators(&mut creators);

    let grouping_query = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(
            Condition::any()
                .add(asset_grouping::Column::Verified.eq(true))
                // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
                // Therefore if verified is null, we can assume that the group is verified.
                .add(asset_grouping::Column::Verified.is_null()),
        )
        .order_by_asc(asset_grouping::Column::AssetId);

    let groups = if options.show_collection_metadata {
        let groups = grouping_query.all(conn).await?;

        let group_values = groups
            .iter()
            .filter_map(|group| {
                group
                    .group_value
                    .as_ref()
                    .and_then(|g| bs58::decode(g).into_vec().ok())
            })
            .collect::<Vec<_>>();

        let asset_data = asset_data::Entity::find()
            .filter(asset_data::Column::Id.is_in(group_values))
            .order_by_asc(asset_data::Column::Id)
            .all(conn)
            .await?;

        let asset_data_map: HashMap<_, _, RandomState> = HashMap::from_iter(
            asset_data
                .into_iter()
                .map(|ad| (ad.id.clone(), ad))
                .collect::<Vec<_>>(),
        );

        let mut groups_tup = Vec::new();
        for g in groups.into_iter() {
            let a = g.group_value.as_ref().and_then(|g| {
                bs58::decode(g)
                    .into_vec()
                    .ok()
                    .and_then(|v| asset_data_map.get(&v))
                    .cloned()
            });

            groups_tup.push((g, a));
        }

        groups_tup
    } else {
        grouping_query
            .all(conn)
            .await?
            .into_iter()
            .map(|g| (g, None))
            .collect::<Vec<_>>()
    };

    Ok(FullAsset {
        asset,
        data,
        authorities,
        creators,
        inscription,
        groups,
        token_info,
    })
}

pub async fn fetch_transactions(
    conn: &impl ConnectionTrait,
    tree: Vec<u8>,
    leaf_idx: i64,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // Default sort direction is Desc
    // Similar to GetSignaturesForAddress in the Solana API
    let sort_direction = sort_direction.unwrap_or(AssetSortDirection::Desc);
    let sort_order = match sort_direction {
        AssetSortDirection::Asc => sea_orm::Order::Asc,
        AssetSortDirection::Desc => sea_orm::Order::Desc,
    };

    let mut stmt = cl_audits_v2::Entity::find().filter(cl_audits_v2::Column::Tree.eq(tree));
    stmt = stmt.filter(cl_audits_v2::Column::LeafIdx.eq(leaf_idx));
    stmt = stmt.order_by(cl_audits_v2::Column::Seq, sort_order.clone());

    stmt = paginate(
        pagination,
        limit,
        stmt,
        sort_order,
        cl_audits_v2::Column::Seq,
    );
    let transactions = stmt.all(conn).await?;
    let transaction_list = transactions
        .into_iter()
        .map(|transaction| {
            let tx = bs58::encode(transaction.tx).into_string();
            let ix = Instruction::to_pascal_case(&transaction.instruction).to_string();
            (tx, ix)
        })
        .collect();

    Ok(transaction_list)
}

pub async fn get_asset_signatures(
    conn: &impl ConnectionTrait,
    asset_id: Option<Vec<u8>>,
    tree_id: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // if tree_id and leaf_idx are provided, use them directly to fetch transactions
    if let (Some(tree_id), Some(leaf_idx)) = (tree_id, leaf_idx) {
        let transactions =
            fetch_transactions(conn, tree_id, leaf_idx, pagination, limit, sort_direction).await?;
        return Ok(transactions);
    }

    if asset_id.is_none() {
        return Err(DbErr::Custom(
            "Either 'id' or both 'tree' and 'leafIndex' must be provided".to_string(),
        ));
    }

    // if only asset_id is provided, fetch the latest tree and leaf_idx (asset.nonce) for the asset
    // and use them to fetch transactions
    let stmt = asset::Entity::find()
        .distinct_on([(asset::Entity, asset::Column::Id)])
        .filter(asset::Column::Id.eq(asset_id))
        .order_by(asset::Column::Id, Order::Desc)
        .limit(1);
    let asset = stmt.one(conn).await?;
    if let Some(asset) = asset {
        let tree = asset
            .tree_id
            .ok_or(DbErr::RecordNotFound("Tree not found".to_string()))?;
        if tree.is_empty() {
            return Err(DbErr::Custom("Empty tree for asset".to_string()));
        }
        let leaf_idx = asset
            .nonce
            .ok_or(DbErr::RecordNotFound("Leaf ID does not exist".to_string()))?;
        let transactions =
            fetch_transactions(conn, tree, leaf_idx, pagination, limit, sort_direction).await?;
        Ok(transactions)
    } else {
        Ok(Vec::new())
    }
}

fn filter_out_stale_creators(creators: &mut Vec<asset_creators::Model>) {
    // If the first creator is an empty Vec, it means the creator array is empty (which is allowed
    // for compressed assets in Bubblegum).
    if !creators.is_empty() && creators[0].creator.is_empty() {
        creators.clear();
    } else {
        // For both compressed and non-compressed assets, any creators that do not have the max
        // `slot_updated` value are stale and should be removed.
        let max_slot_updated = creators.iter().map(|creator| creator.slot_updated).max();
        if let Some(max_slot_updated) = max_slot_updated {
            creators.retain(|creator| creator.slot_updated == max_slot_updated);
        }

        // For compressed assets, any creators that do not have the max `seq` value are stale and
        // should be removed.  A `seq` value of 0 indicates a decompressed or never-compressed
        // asset.  So if a `seq` value of 0 is present, then all creators with nonzero `seq` values
        // are stale and should be removed.
        let seq = if creators
            .iter()
            .map(|creator| creator.seq)
            .any(|seq| seq == Some(0))
        {
            Some(Some(0))
        } else {
            creators.iter().map(|creator| creator.seq).max()
        };

        if let Some(seq) = seq {
            creators.retain(|creator| creator.seq == seq);
        }
    }
}

pub async fn get_token_accounts(
    conn: &impl ConnectionTrait,
    owner_address: Option<Vec<u8>>,
    mint_address: Option<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<token_accounts::Model>, DbErr> {
    let mut condition = Condition::all();

    if options.show_zero_balance {
        condition = condition.add(token_accounts::Column::Amount.gte(0));
    } else {
        condition = condition.add(token_accounts::Column::Amount.gt(0));
    }

    if owner_address.is_none() && mint_address.is_none() {
        return Err(DbErr::Custom(
            "Either 'ownerAddress' or 'mintAddress' must be provided".to_string(),
        ));
    }

    if let Some(owner) = owner_address {
        condition = condition.add(token_accounts::Column::Owner.eq(owner));
    }
    if let Some(mint) = mint_address {
        condition = condition.add(token_accounts::Column::Mint.eq(mint));
    }

    let token_accounts = paginate(
        pagination,
        limit,
        token_accounts::Entity::find().filter(condition),
        Order::Asc,
        token_accounts::Column::Pubkey,
    )
    .all(conn)
    .await?;

    Ok(token_accounts)
}

pub async fn get_token_largest_accounts(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponseAndContext<Vec<RpcTokenAccountBalance>>, DbErr> {
    let mint_acc = tokens::Entity::find()
        .filter(tokens::Column::Mint.eq(mint_address.clone()))
        .one(conn)
        .await?
        .ok_or(DbErr::RecordNotFound("Mint Account Not Found".to_string()))?;

    let largest_token_accounts = token_accounts::Entity::find()
        .filter(token_accounts::Column::Mint.eq(mint_address))
        .order_by_desc(token_accounts::Column::Amount)
        .limit(20) // Select the top 20 largest token accounts
        .all(conn)
        .await?;

    let value = largest_token_accounts
        .into_iter()
        .map(|ta| {
            let ui_amount: f64 = (ta.amount as f64).div(10u64.pow(mint_acc.decimals as u32) as f64);
            RpcTokenAccountBalance {
                address: bs58::encode(ta.pubkey).into_string(),
                amount: UiTokenAmount {
                    ui_amount: Some(ui_amount),
                    decimals: mint_acc.decimals as u8,
                    amount: ta.amount.to_string(),
                    ui_amount_string: ui_amount.to_string(),
                },
            }
        })
        .collect();

    Ok(SolanaRpcResponseAndContext {
        value,
        context: SolanaRpcContext::default(),
    })
}

pub async fn get_token_supply(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponseAndContext<RpcTokenSupply>, DbErr> {
    let token = tokens::Entity::find()
        .filter(tokens::Column::Mint.eq(mint_address))
        .one(conn)
        .await?
        .ok_or(DbErr::RecordNotFound("Token Not Found".to_string()))?;

    let ui_supply = token
        .supply
        .to_f64()
        .map_or(0f64, |s| s.div(10u64.pow(token.decimals as u32) as f64));

    let value = RpcTokenSupply {
        amount: token.supply.to_string(),
        decimals: token.decimals as u8,
        ui_amount: Some(ui_supply),
        ui_amount_string: ui_supply.to_string(),
    };

    Ok(SolanaRpcResponseAndContext {
        value,
        context: SolanaRpcContext::default(),
    })
}

fn get_edition_data_from_json<T: DeserializeOwned>(data: Value) -> Result<T, DbErr> {
    serde_json::from_value(data).map_err(|e| DbErr::Custom(e.to_string()))
}

fn attachment_to_nft_edition(
    attachment: asset_v1_account_attachments::Model,
) -> Result<NftEdition, DbErr> {
    let data: Edition = attachment
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound("Edition data not found".to_string()))
        .map(get_edition_data_from_json)??;

    Ok(NftEdition {
        mint_address: attachment
            .asset_id
            .clone()
            .map(|id| bs58::encode(id).into_string())
            .unwrap_or("".to_string()),
        edition_number: data.edition,
        edition_address: bs58::encode(attachment.id.clone()).into_string(),
    })
}

pub async fn get_nft_editions(
    conn: &impl ConnectionTrait,
    mint_address: Pubkey,
    pagination: &Pagination,
    limit: u64,
) -> Result<NftEditions, DbErr> {
    let master_edition_pubkey = MasterEdition::find_pda(&mint_address).0;

    // to fetch nft editions associated with a mint we need to fetch the master edition first
    let master_edition =
        asset_v1_account_attachments::Entity::find_by_id(master_edition_pubkey.to_bytes().to_vec())
            .one(conn)
            .await?
            .ok_or(DbErr::RecordNotFound(
                "Master Edition not found".to_string(),
            ))?;

    let master_edition_data: MasterEdition = master_edition
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound(
            "Master Edition data not found".to_string(),
        ))
        .map(get_edition_data_from_json)??;

    let mut stmt = asset_v1_account_attachments::Entity::find();

    stmt = stmt.filter(
        asset_v1_account_attachments::Column::AttachmentType
            .eq(V1AccountAttachments::Edition)
            // The data field is a JSON field that contains the edition data.
            .and(asset_v1_account_attachments::Column::Data.is_not_null())
            // The parent field is a string field that contains the master edition pubkey ( mapping edition to master edition )
            .and(Expr::cust(&format!(
                "data->>'parent' = '{}'",
                master_edition_pubkey
            ))),
    );

    let nft_editions = paginate(
        pagination,
        limit,
        stmt,
        Order::Asc,
        asset_v1_account_attachments::Column::Id,
    )
    .all(conn)
    .await?
    .into_iter()
    .map(attachment_to_nft_edition)
    .collect::<Result<Vec<NftEdition>, _>>()?;

    let (page, before, after, cursor) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft, None)
        }
        Pagination::Page { page } => (Some(*page as u32), None, None, None),
        Pagination::Cursor(_) => {
            if let Some(last_asset) = nft_editions.last() {
                let cursor_str = last_asset.edition_address.clone();
                (None, None, None, Some(cursor_str))
            } else {
                (None, None, None, None)
            }
        }
    };

    Ok(NftEditions {
        total: nft_editions.len() as u32,
        master_edition_address: master_edition_pubkey.to_string(),
        supply: master_edition_data.supply,
        max_supply: master_edition_data.max_supply,
        editions: nft_editions,
        limit: limit as u32,
        page,
        before,
        after,
        cursor,
    })
}

async fn get_inscription_by_mint(
    conn: &impl ConnectionTrait,
    mint: Vec<u8>,
) -> Result<asset_v1_account_attachments::Model, DbErr> {
    asset_v1_account_attachments::Entity::find()
        .filter(
            asset_v1_account_attachments::Column::Data
                .is_not_null()
                .and(Expr::cust(&format!(
                    "data->>'root' = '{}'",
                    bs58::encode(mint).into_string()
                ))),
        )
        .one(conn)
        .await
        .and_then(|o| match o {
            Some(t) => Ok(t),
            _ => Err(DbErr::RecordNotFound("Inscription Not Found".to_string())),
        })
}

async fn find_tokens(
    conn: &impl ConnectionTrait,
    ids: Vec<Vec<u8>>,
) -> Result<Vec<tokens::Model>, DbErr> {
    tokens::Entity::find()
        .filter(tokens::Column::Mint.is_in(ids))
        .all(conn)
        .await
        .map_err(|_| DbErr::RecordNotFound("Token (s) Not Found".to_string()))
}
