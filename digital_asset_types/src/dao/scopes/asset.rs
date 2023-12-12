use crate::dao::{
    asset::{self},
    asset_authority, asset_creators, asset_data, asset_grouping, Cursor, FullAsset, GroupingSize,
    Pagination,
};

use indexmap::IndexMap;
use sea_orm::{entity::*, query::*, ConnectionTrait, DbErr, Order};
use std::collections::HashMap;

pub fn paginate<'db, T, C>(
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

pub async fn get_by_creator(
    conn: &impl ConnectionTrait,
    creator: Vec<u8>,
    only_verified: bool,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    show_unverified_collections: bool,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = Condition::all()
        .add(asset_creators::Column::Creator.eq(creator))
        .add(asset::Column::Supply.gt(0));
    if only_verified {
        condition = condition.add(asset_creators::Column::Verified.eq(true));
    }
    get_by_related_condition(
        conn,
        condition,
        asset::Relation::AssetCreators,
        sort_by,
        sort_direction,
        pagination,
        limit,
        show_unverified_collections,
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

pub async fn get_by_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    show_unverified_collections: bool,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = asset_grouping::Column::GroupKey
        .eq(group_key)
        .and(asset_grouping::Column::GroupValue.eq(group_value));

    if !show_unverified_collections {
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
        asset::Relation::AssetGrouping,
        sort_by,
        sort_direction,
        pagination,
        limit,
        show_unverified_collections,
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
    show_unverified_collections: bool,
) -> Result<Vec<FullAsset>, DbErr> {
    let cond = Condition::all()
        .add(asset::Column::Owner.eq(owner))
        .add(asset::Column::Supply.gt(0));
    get_assets_by_condition(
        conn,
        cond,
        vec![],
        sort_by,
        sort_direction,
        pagination,
        limit,
        show_unverified_collections,
    )
    .await
}

pub async fn get_assets(
    conn: &impl ConnectionTrait,
    asset_ids: Vec<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
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
        false,
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
    show_unverified_collections: bool,
) -> Result<Vec<FullAsset>, DbErr> {
    let cond = Condition::all()
        .add(asset_authority::Column::Authority.eq(authority))
        .add(asset::Column::Supply.gt(0));
    get_by_related_condition(
        conn,
        cond,
        asset::Relation::AssetAuthority,
        sort_by,
        sort_direction,
        pagination,
        limit,
        show_unverified_collections,
    )
    .await
}

async fn get_by_related_condition<E>(
    conn: &impl ConnectionTrait,
    condition: Condition,
    relation: E,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    show_unverified_collections: bool,
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
    get_related_for_assets(conn, assets, show_unverified_collections).await
}

pub async fn get_related_for_assets(
    conn: &impl ConnectionTrait,
    assets: Vec<asset::Model>,
    show_unverified_collections: bool,
) -> Result<Vec<FullAsset>, DbErr> {
    let asset_ids = assets.iter().map(|a| a.id.clone()).collect::<Vec<_>>();

    let asset_data: Vec<asset_data::Model> = asset_data::Entity::find()
        .filter(asset_data::Column::Id.is_in(asset_ids))
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
            };
            acc.insert(id, fa);
        };
        acc
    });
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

    let creators = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.is_in(ids.clone()))
        .order_by_asc(asset_creators::Column::AssetId)
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;
    for c in creators.into_iter() {
        if let Some(asset) = assets_map.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    let cond = if show_unverified_collections {
        Condition::all()
    } else {
        Condition::any()
            .add(asset_grouping::Column::Verified.eq(true))
            // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
            // Therefore if verified is null, we can assume that the group is verified.
            .add(asset_grouping::Column::Verified.is_null())
    };

    let grouping = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.is_in(ids.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(cond)
        .order_by_asc(asset_grouping::Column::AssetId)
        .all(conn)
        .await?;
    for g in grouping.into_iter() {
        if let Some(asset) = assets_map.get_mut(&g.asset_id) {
            asset.groups.push(g);
        }
    }

    Ok(assets_map.into_iter().map(|(_, v)| v).collect())
}

pub async fn get_assets_by_condition(
    conn: &impl ConnectionTrait,
    condition: Condition,
    joins: Vec<RelationDef>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    show_unverified_collections: bool,
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
    let full_assets = get_related_for_assets(conn, assets, show_unverified_collections).await?;
    Ok(full_assets)
}

pub async fn get_by_id(
    conn: &impl ConnectionTrait,
    asset_id: Vec<u8>,
    include_no_supply: bool,
) -> Result<FullAsset, DbErr> {
    let mut asset_data =
        asset::Entity::find_by_id(asset_id.clone()).find_also_related(asset_data::Entity);
    if !include_no_supply {
        asset_data = asset_data.filter(Condition::all().add(asset::Column::Supply.gt(0)));
    }
    let asset_data: (asset::Model, asset_data::Model) =
        asset_data.one(conn).await.and_then(|o| match o {
            Some((a, Some(d))) => Ok((a, d)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })?;

    let (asset, data) = asset_data;
    let authorities: Vec<asset_authority::Model> = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_authority::Column::AssetId)
        .all(conn)
        .await?;
    let creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;
    let grouping: Vec<asset_grouping::Model> = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(
            Condition::any()
                .add(asset_grouping::Column::Verified.eq(true))
                // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
                // Therefore if verified is null, we can assume that the group is verified.
                .add(asset_grouping::Column::Verified.is_null()),
        )
        .order_by_asc(asset_grouping::Column::AssetId)
        .all(conn)
        .await?;
    Ok(FullAsset {
        asset,
        data,
        authorities,
        creators,
        groups: grouping,
    })
}
