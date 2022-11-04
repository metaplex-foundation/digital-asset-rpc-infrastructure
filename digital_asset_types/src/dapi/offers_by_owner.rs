use crate::dao::asset;
use crate::dao::prelude::AssetData;
use crate::rpc::filter::OfferSorting;
use crate::rpc::response::OfferList;
use crate::rpc::Offer;
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

pub async fn get_offers_by_owner(
    db: &DatabaseConnection,
    owner_address: Vec<u8>,
    _sort_by: OfferSorting,
    limit: u32,
    page: u32,
    before: Vec<u8>,
    after: Vec<u8>,
) -> Result<OfferList, DbErr> {
    let assets = if page > 0 {
        let paginator = asset::Entity::find()
            .filter(Condition::all().add(asset::Column::Owner.eq(owner_address.clone())))
            .find_also_related(AssetData)
            // .order_by_asc(sort_column)
            .paginate(db, limit.try_into().unwrap());

        paginator.fetch_page((page - 1).try_into().unwrap()).await?
    } else if !before.is_empty() {
        let rows = asset::Entity::find()
            // .order_by_asc(sort_column)
            .filter(asset::Column::Owner.eq(owner_address.clone()))
            .cursor_by(asset::Column::Id)
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
            // .order_by_asc(sort_column)
            .filter(asset::Column::Owner.eq(owner_address.clone()))
            .cursor_by(asset::Column::Id)
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

    let filter_assets: Result<Vec<_>, _> = assets
        .into_iter()
        .map(|(asset, asset_data)| match asset_data {
            Some(asset_data) => Ok((asset, asset_data)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })
        .collect();
    let build_listings_list = filter_assets?.into_iter().map(|_asset| async move {
        Offer {
            from: todo!(),
            amount: todo!(),
            price: todo!(),
            market_id: todo!(),
        }
    });

    let built_assets = futures::future::join_all(build_listings_list).await;

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

    Ok(OfferList {
        total,
        limit,
        page,
        before,
        after,
        items: built_assets,
    })
}
