use crate::dao::prelude::AssetData;
use crate::dao::{asset, asset_authority, asset_creators, asset_grouping};
use crate::dapi::asset::{get_content, to_authority, to_creators, to_grouping};
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use crate::rpc::{Asset as RpcAsset, Compression, Interface, Ownership, Royalty};
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};
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

    let num_conditions: usize = search_assets_query.count_conditions();

    if num_conditions == 0 {
        todo!("return error")
    }

    let conditions: Condition = search_assets_query.conditions();

    let assets = if page > 0 {
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

    let filter_assets: Result<Vec<_>, _> = assets
        .into_iter()
        .map(|(asset, asset_data)| match asset_data {
            Some(asset_data) => Ok((asset, asset_data)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })
        .collect();

    let build_asset_list = filter_assets?
        .into_iter()
        .map(|(asset, asset_data)| async move {
            let interface = match asset.specification_version {
                1 => Interface::NftOneZero,
                _ => Interface::Nft,
            };

            let content = get_content(&asset, &asset_data).unwrap();

            let authorities = asset_authority::Entity::find()
                .filter(asset_authority::Column::AssetId.eq(asset.id.clone()))
                .all(db)
                .await
                .unwrap();

            let creators = asset_creators::Entity::find()
                .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
                .all(db)
                .await
                .unwrap();

            let grouping = asset_grouping::Entity::find()
                .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
                .all(db)
                .await
                .unwrap();

            let rpc_authorities = to_authority(authorities);
            let rpc_creators = to_creators(creators);
            let rpc_groups = to_grouping(grouping);

            RpcAsset {
                interface,
                id: bs58::encode(asset.id).into_string(),
                content: Some(content),
                authorities: Some(rpc_authorities),
                compression: Some(Compression {
                    eligible: asset.compressible,
                    compressed: asset.compressed,
                }),
                grouping: Some(rpc_groups),
                royalty: Some(Royalty {
                    royalty_model: asset.royalty_target_type.into(),
                    target: asset.royalty_target.map(|s| bs58::encode(s).into_string()),
                    percent: (asset.royalty_amount as f64) * 0.0001,
                    locked: false,
                }),
                creators: Some(rpc_creators),
                ownership: Ownership {
                    frozen: asset.frozen,
                    delegated: asset.delegate.is_some(),
                    delegate: asset.delegate.map(|s| bs58::encode(s).into_string()),
                    ownership_model: asset.owner_type.into(),
                    owner: bs58::encode(asset.owner).into_string(),
                },
            }
        });

    let built_assets = futures::future::join_all(build_asset_list).await;

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
                .add_option(self.chain_data_id.map(|x| asset::Column::ChainDataId.eq(x)))
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
                .add_option(self.chain_data_id.map(|x| asset::Column::ChainDataId.eq(x)))
                .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)))
                .add_option(self.seq.map(|x| asset::Column::Seq.eq(x))),
        };

        match self.negate {
            None | Some(false) => conditions,
            Some(true) => conditions.not(),
        }
    }
}
