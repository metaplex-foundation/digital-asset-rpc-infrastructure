use std::collections::HashMap;
use crate::dao::prelude::AssetData;
use crate::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping, FullAsset, FullAssetList, sea_orm_active_enums::*};
use crate::dapi::asset::{asset_list_to_rpc, get_asset_list_data, get_content, get_interface, to_authority, to_creators, to_grouping};
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use crate::rpc::{Asset as RpcAsset, Compression, Interface, Ownership, Royalty};
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
use crate::dao::cl_items::Column::Hash;
use crate::rpc::Scope::Full;

pub async fn get_assets_by_creator(
    db: &DatabaseConnection,
    creator_expression: Vec<Vec<u8>>,
    sort_by: AssetSorting,
    limit: u32,
    page: u32,
    before: Vec<u8>,
    after: Vec<u8>,
) -> Result<AssetList, DbErr> {
    let sort_column = match sort_by {
        AssetSorting::Created => asset::Column::CreatedAt,
        AssetSorting::Updated => todo!(),
        AssetSorting::RecentAction => todo!(),
    };

    let mut conditions = Condition::any();
    for creator in creator_expression {
        conditions = conditions.add(asset_creators::Column::Creator.eq(creator.clone()));
    }

    let assets: Vec<(asset::Model, Option<asset_data::Model>)> = if page > 0 {
        let paginator = asset::Entity::find()
            .join(
                JoinType::LeftJoin,
                asset::Entity::has_many(asset_creators::Entity).into(),
            )
            .filter(conditions)
            .find_also_related(AssetData)
            .order_by_asc(sort_column)
            .paginate(db, limit.try_into().unwrap());

        paginator.fetch_page((page - 1).try_into().unwrap()).await?
    } else if !before.is_empty() {
        let rows = asset::Entity::find()
            .order_by_asc(sort_column)
            .join(
                JoinType::LeftJoin,
                asset::Entity::has_many(asset_creators::Entity).into(),
            )
            .filter(conditions)
            .cursor_by(asset_creators::Column::AssetId)
            .before(before.clone())
            .first(limit.into())
            .all(db)
            .await?
            .into_iter()
            .map(|x| async move {
                let asset_data = x.find_related(AssetData).one(db).await.unwrap();

                (x, asset_data)
            });

        let assets = futures::future::join_all(rows).await;
        assets
    } else {
        let rows = asset::Entity::find()
            .order_by_asc(sort_column)
            .join(
                JoinType::LeftJoin,
                asset::Entity::has_many(asset_creators::Entity).into(),
            )
            .filter(conditions)
            .cursor_by(asset_creators::Column::AssetId)
            .after(after.clone())
            .first(limit.into())
            .all(db)
            .await?
            .into_iter()
            .map(|x| async move {
                let asset_data = x.find_related(AssetData).one(db).await.unwrap();

                (x, asset_data)
            });

        let assets = futures::future::join_all(rows).await;
        assets
    };

    let built_assets = get_asset_list_data( db, assets).await?;
    let total = built_assets.len() as u32;
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

    Ok(AssetList {
        total,
        limit,
        page,
        before,
        after,
        items: built_assets,
    })
}
