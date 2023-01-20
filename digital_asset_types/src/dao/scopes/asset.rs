use crate::dao::{
    asset, asset_authority, asset_creators, asset_data, asset_grouping, FullAsset, Pagination,
};
use sea_orm::{
    entity::*,
    query::*,
    sea_query::{ColumnRef, IntoColumnRef, TableRef},
    ConnectionTrait, DbErr, Order, Value, DbBackend,
};
use std::collections::BTreeMap;

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
    creators: Vec<Vec<u8>>,
    only_verified: bool,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
) -> Result<Vec<FullAsset>, DbErr> {
    if creators.is_empty() {
        return Ok(vec![]);
    }
    if creators.len() > 5 {
        return Err(DbErr::Custom("Too many creators".to_string()));
    }
    let mut condition = Condition::any();
    for creator in creators {
        condition = condition.add(asset_creators::Column::Creator.eq(creator));
    }
    if only_verified {
        condition = Condition::all()
            .add(condition)
            .add(asset_creators::Column::Verified.eq(true));
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
        Condition::all().add(condition),
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
    get_assets_by_column(
        conn,
        owner,
        asset::Column::Owner,
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
    get_by_related_condition(
        conn,
        Condition::all().add(asset_authority::Column::Authority.eq(authority)),
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
        .find_also_related(asset_data::Entity)
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
    assets: Vec<(asset::Model, Option<asset_data::Model>)>,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut ids = Vec::with_capacity(assets.len());
    // Using BTreeMap to preserve order.
    let mut assets_map = assets.into_iter().fold(BTreeMap::new(), |mut x, asset| {
        if let Some(ad) = asset.1 {
            let id = asset.0.id.clone();
            let fa = FullAsset {
                asset: asset.0,
                data: ad,
                authorities: vec![],
                creators: vec![],
                groups: vec![],
            };

            x.insert(id.clone(), fa);
            ids.push(id);
        }
        x
    });

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

pub async fn get_assets_by_column(
    conn: &impl ConnectionTrait,
    target_value: impl Into<Value>,
    target_column: asset::Column,
    sort_by: asset::Column,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
) -> Result<Vec<FullAsset>, DbErr> {
    get_assets_by_condition(
        conn,
        Condition::all().add(target_column.eq(target_value)),
        vec![],
        sort_by,
        sort_direction,
        pagination,
        limit,
    )
    .await
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
    let mut stmt = asset::Entity::find()
        
        .distinct_on([(asset::Entity, asset::Column::Id)]);
    for def in joins {
        stmt = stmt.join(JoinType::LeftJoin, def);
    }
    println!("SLQL::{} " , stmt.build(DbBackend::Postgres).sql);
    stmt = stmt
        .filter(condition)
        .order_by(asset::Column::Id, Order::Desc)
        .order_by(sort_by, sort_direction);

        println!("SLQL::{} " , stmt.build(DbBackend::Postgres).sql);

    stmt = paginate(pagination, limit, stmt);

    println!("SLQL::{} " , stmt.build(DbBackend::Postgres).sql);
    let asset_list = stmt.find_also_related(asset_data::Entity).all(conn).await?;
    get_related_for_assets(conn, asset_list).await
}

pub async fn get_by_id(conn: &impl ConnectionTrait, asset_id: Vec<u8>) -> Result<FullAsset, DbErr> {
    let asset_data: (asset::Model, asset_data::Model) = asset::Entity::find_by_id(asset_id)
        .find_also_related(asset_data::Entity)
        .one(conn)
        .await
        .and_then(|o| match o {
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
