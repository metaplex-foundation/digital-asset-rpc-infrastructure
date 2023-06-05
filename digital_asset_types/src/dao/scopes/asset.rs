use crate::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping, FullAsset, GroupingSize,
    Pagination,
};
use sea_orm::{entity::*, query::*, ConnectionTrait, DbErr, Order};
use std::collections::{BTreeMap, HashMap};

pub fn paginate<'db, T>(pagination: &Pagination, limit: u64, stmt: T) -> T
where
    T: QueryFilter + QuerySelect,
{
    let mut stmt = stmt;
    match pagination {
        Pagination::Keyset { before, after } => {
            if let Some(b) = before {
                stmt = stmt.filter(asset::Column::Id.lt(b.clone()));
            }
            if let Some(a) = after {
                stmt = stmt.filter(asset::Column::Id.gt(a.clone()));
            }
        }
        Pagination::Page { page } => {
            if *page > 0 {
                stmt = stmt.offset((page - 1) * limit)
            }
        }
    }
    stmt.limit(limit)
}

pub async fn get_by_creator(
    conn: &impl ConnectionTrait,
    creator: Vec<u8>,
    only_verified: bool,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
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
                .add(asset_grouping::Column::GroupValue.eq(group_value)),
        )
        .count(conn)
        .await?;
    Ok(GroupingSize { size })
}

pub async fn get_by_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
) -> Result<Vec<FullAsset>, DbErr> {
    let condition = asset_grouping::Column::GroupKey
        .eq(group_key)
        .and(asset_grouping::Column::GroupValue.eq(group_value));
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
    )
    .await
}

pub async fn get_assets_by_owner(
    conn: &impl ConnectionTrait,
    owner: Vec<u8>,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
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
    )
    .await
}

pub async fn get_by_authority(
    conn: &impl ConnectionTrait,
    authority: Vec<u8>,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
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
    )
    .await
}

async fn get_by_related_condition<E>(
    conn: &impl ConnectionTrait,
    condition: Condition,
    relation: E,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
) -> Result<Vec<FullAsset>, DbErr>
where
    E: RelationTrait,
{
    let mut stmt = asset::Entity::find()
        .filter(condition)
        .join(JoinType::LeftJoin, relation.def())
        .distinct_on([(asset::Entity, asset::Column::Id)])
        .order_by(asset::Column::Id, Order::Desc)
        .order_by(sort_by, sort_direction);

    stmt = paginate(pagination, limit, stmt);

    let assets = stmt.all(conn).await?;

    get_related_for_assets(conn, assets).await
}

pub async fn get_related_for_assets(
    conn: &impl ConnectionTrait,
    assets: Vec<asset::Model>,
) -> Result<Vec<FullAsset>, DbErr> {
    let asset_ids = assets.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    let asset_data: Vec<asset_data::Model> = asset_data::Entity::find()
        .filter(asset_data::Column::Id.is_in(asset_ids))
        .all(conn)
        .await?;

    let asset_data_map = asset_data.into_iter().fold(HashMap::new(), |mut x, ad| {
        x.insert(ad.id.clone(), ad);
        x
    });

    // Using BTreeMap to preserve order.
    let mut assets_map = assets.into_iter().fold(BTreeMap::new(), |mut x, asset| {
        if let Some(ad) = asset_data_map.get(&asset.id) {
            let id = asset.id.clone();
            let fa = FullAsset {
                asset: asset,
                data: ad.clone(),
                authorities: vec![],
                creators: vec![],
                groups: vec![],
            };
            x.insert(id.clone(), fa);
        }
        x
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
        .all(conn)
        .await?;
    for c in creators.into_iter() {
        if let Some(asset) = assets_map.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    let grouping = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.is_in(ids.clone()))
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
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut stmt = asset::Entity::find().distinct_on([(asset::Entity, asset::Column::Id)]);
    for def in joins {
        stmt = stmt.join(JoinType::LeftJoin, def);
    }
    stmt = stmt
        .filter(condition)
        .order_by(asset::Column::Id, Order::Desc)
        .order_by(sort_by, sort_direction);

    stmt = paginate(pagination, limit, stmt);
    let asset_list = stmt.all(conn).await?;
    get_related_for_assets(conn, asset_list).await
}

pub async fn get_by_id(
    conn: &impl ConnectionTrait,
    asset_id: Vec<u8>,
    include_no_supply: bool,
) -> Result<FullAsset, DbErr> {
    let mut asset_data = asset::Entity::find_by_id(asset_id).find_also_related(asset_data::Entity);
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
        .all(conn)
        .await?;
    let creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
        .all(conn)
        .await?;
    let grouping: Vec<asset_grouping::Model> = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
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
