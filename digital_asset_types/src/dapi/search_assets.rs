use crate::dao::prelude::AssetData;
use crate::dao::{asset, asset_creators, asset_data};
use crate::dapi::asset::get_asset_list_data;
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr};
use serde::Deserialize;

pub async fn search_assets(
    db: &DatabaseConnection,
    search_assets_query: SearchAssetsQuery,
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

    if search_assets_query.count_conditions() == 0 {
        return Err(DbErr::Custom(
            "No search conditions were provided".to_string(),
        ));
    }

    let conditions: Condition = search_assets_query.conditions();

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
            .order_by_asc(sort_column)
            .join(
                JoinType::LeftJoin,
                asset::Entity::has_many(asset_creators::Entity).into(),
            )
            .filter(conditions)
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

    let built_assets = get_asset_list_data(db, assets).await?;
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

#[derive(Deserialize, Debug)]
pub struct SearchAssetsQuery {
    // fn def(&self) -> ColumnDef {
    //     match self {
    //         Self::Id => ColumnType::Binary.def(),
    // done    Self::SpecificationVersion => ColumnType::Integer.def(),
    //         Self::Owner => ColumnType::Binary.def(),
    //         Self::OwnerType => OwnerType::db_type(),
    //         Self::Delegate => ColumnType::Binary.def().null(),
    //         Self::Frozen => ColumnType::Boolean.def(),
    //         Self::Supply => ColumnType::BigInteger.def(),
    //         Self::SupplyMint => ColumnType::Binary.def().null(),
    //         Self::Compressed => ColumnType::Boolean.def(),
    // done    Self::Compressible => ColumnType::Boolean.def(),
    //         Self::TreeId => ColumnType::Binary.def().null(),
    //         Self::Leaf => ColumnType::Binary.def().null(),
    //         Self::Nonce => ColumnType::BigInteger.def(),
    //         Self::RoyaltyTargetType => RoyaltyTargetType::db_type(),
    //         Self::RoyaltyTarget => ColumnType::Binary.def().null(),
    //         Self::RoyaltyAmount => ColumnType::Integer.def(),
    //         Self::ChainDataId => ColumnType::BigInteger.def().null(),
    //         Self::CreatedAt => ColumnType::TimestampWithTimeZone.def().null(),
    //         Self::Burnt => ColumnType::Boolean.def(),
    //         Self::Seq => ColumnType::BigInteger.def(),
    //     }
    // }

    // Conditions
    negate: Option<bool>,

    /// Defaults to [ConditionType::All]
    condition_type: Option<ConditionType>,

    // Asset columns
    specification_verison: Option<u64>,
    frozen: Option<bool>,
    supply: Option<u64>,
    compressed: Option<bool>,
    compressible: Option<bool>,
    nonce: Option<u64>,
    royalty_amount: Option<u32>,
    chain_data_id: Option<u64>,
    burnt: Option<bool>,
    seq: Option<u64>,
}

#[derive(Deserialize, Debug)]
enum ConditionType {
    Any,
    All,
}

impl SearchAssetsQuery {
    pub fn count_conditions(&self) -> usize {
        // Initialize counter
        let mut num_conditions = 0;

        // Increment for each condition
        if self.specification_verison.is_some() {
            num_conditions += 1;
        }
        if self.frozen.is_some() {
            num_conditions += 1;
        }
        if self.supply.is_some() {
            num_conditions += 1;
        }
        if self.compressed.is_some() {
            num_conditions += 1;
        }
        if self.compressible.is_some() {
            num_conditions += 1;
        }
        if self.nonce.is_some() {
            num_conditions += 1;
        }
        if self.royalty_amount.is_some() {
            num_conditions += 1;
        }
        if self.chain_data_id.is_some() {
            num_conditions += 1;
        }
        if self.burnt.is_some() {
            num_conditions += 1;
        }
        if self.seq.is_some() {
            num_conditions += 1;
        }

        num_conditions
    }

    pub fn conditions(&self) -> Condition {
        let conditions = match self.condition_type {
            // None --> default to all when no option is provided
            None | Some(ConditionType::All) => Condition::all()
                .add_option(
                    self.specification_verison
                        .map(|x| asset::Column::SpecificationVersion.eq(x)),
                )
                .add_option(self.frozen.map(|x| asset::Column::Frozen.eq(x)))
                .add_option(self.supply.map(|x| asset::Column::Supply.eq(x)))
                .add_option(self.compressed.map(|x| asset::Column::Compressed.eq(x)))
                .add_option(self.compressible.map(|x| asset::Column::Compressible.eq(x)))
                .add_option(self.nonce.map(|x| asset::Column::Nonce.eq(x)))
                .add_option(
                    self.royalty_amount
                        .map(|x| asset::Column::RoyaltyAmount.eq(x)),
                )
                .add_option(self.chain_data_id.map(|x| asset::Column::AssetData.eq(x)))
                .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)))
                .add_option(self.seq.map(|x| asset::Column::Seq.eq(x))),

            Some(ConditionType::Any) => Condition::any()
                .add_option(
                    self.specification_verison
                        .map(|x| asset::Column::SpecificationVersion.eq(x)),
                )
                .add_option(self.frozen.map(|x| asset::Column::Frozen.eq(x)))
                .add_option(self.supply.map(|x| asset::Column::Supply.eq(x)))
                .add_option(self.compressed.map(|x| asset::Column::Compressed.eq(x)))
                .add_option(self.compressible.map(|x| asset::Column::Compressible.eq(x)))
                .add_option(self.nonce.map(|x| asset::Column::Nonce.eq(x)))
                .add_option(
                    self.royalty_amount
                        .map(|x| asset::Column::RoyaltyAmount.eq(x)),
                )
                .add_option(self.chain_data_id.map(|x| asset::Column::AssetData.eq(x)))
                .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)))
                .add_option(self.seq.map(|x| asset::Column::Seq.eq(x))),
        };

        match self.negate {
            None | Some(false) => conditions,
            Some(true) => conditions.not(),
        }
    }
}
