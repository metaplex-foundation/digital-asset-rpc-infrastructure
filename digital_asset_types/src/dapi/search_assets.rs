use crate::dao::{
    asset, asset_creators, asset_data,
    prelude::AssetData,
    sea_orm_active_enums::{
        OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
    },
};
use crate::dapi::asset::get_asset_list_data;
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr};
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

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

    let conditions: Condition = search_assets_query.conditions()?;

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
    // Conditions
    negate: Option<bool>,

    /// Defaults to [ConditionType::All]
    condition_type: Option<ConditionType>,

    // Asset columns
    id: Option<String>,
    alt_id: Option<String>,
    specification_version: Option<SpecificationVersions>,
    specification_asset_class: Option<SpecificationAssetClass>,
    owner: Option<String>,
    owner_type: Option<OwnerType>,
    delegate: Option<String>,
    frozen: Option<bool>,
    supply: Option<u64>,
    supply_mint: Option<String>,
    compressed: Option<bool>,
    compressible: Option<bool>,
    seq: Option<u64>,
    tree_id: Option<String>,
    leaf: Option<String>,
    nonce: Option<u64>,
    royalty_target_type: Option<RoyaltyTargetType>,
    royalty_target: Option<String>,
    royalty_amount: Option<u32>,
    asset_data: Option<String>,
    //created_at: timestamp with timezone
    burnt: Option<bool>,
    slot_updated: Option<u64>,
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
        if self.id.is_some() {
            num_conditions += 1;
        }
        if self.alt_id.is_some() {
            num_conditions += 1;
        }
        if self.specification_version.is_some() {
            num_conditions += 1;
        }
        if self.specification_asset_class.is_some() {
            num_conditions += 1;
        }
        if self.owner.is_some() {
            num_conditions += 1;
        }
        if self.owner_type.is_some() {
            num_conditions += 1;
        }
        if self.delegate.is_some() {
            num_conditions += 1;
        }
        if self.frozen.is_some() {
            num_conditions += 1;
        }
        if self.supply.is_some() {
            num_conditions += 1;
        }
        if self.supply_mint.is_some() {
            num_conditions += 1;
        }
        if self.compressed.is_some() {
            num_conditions += 1;
        }
        if self.compressible.is_some() {
            num_conditions += 1;
        }
        if self.seq.is_some() {
            num_conditions += 1;
        }
        if self.tree_id.is_some() {
            num_conditions += 1;
        }
        if self.leaf.is_some() {
            num_conditions += 1;
        }
        if self.nonce.is_some() {
            num_conditions += 1;
        }
        if self.royalty_target_type.is_some() {
            num_conditions += 1;
        }
        if self.royalty_target.is_some() {
            num_conditions += 1;
        }
        if self.royalty_amount.is_some() {
            num_conditions += 1;
        }
        if self.asset_data.is_some() {
            num_conditions += 1;
        }
        if self.burnt.is_some() {
            num_conditions += 1;
        }
        if self.slot_updated.is_some() {
            num_conditions += 1;
        }

        num_conditions
    }

    pub fn conditions(&self) -> Result<Condition, DbErr> {
        let mut conditions = match self.condition_type {
            // None --> default to all when no option is provided
            None | Some(ConditionType::All) => Condition::all(),
            Some(ConditionType::Any) => Condition::any(),
        };

        conditions = conditions
            .add_option(validate_opt_pubkey(&self.id)?.map(|x| asset::Column::Id.eq(x)))
            .add_option(validate_opt_pubkey(&self.alt_id)?.map(|x| asset::Column::AltId.eq(x)))
            .add_option(
                self.specification_version
                    .clone()
                    .map(|x| asset::Column::SpecificationVersion.eq(x)),
            )
            .add_option(
                self.specification_asset_class
                    .clone()
                    .map(|x| asset::Column::SpecificationAssetClass.eq(x)),
            )
            .add_option(validate_opt_pubkey(&self.owner)?.map(|x| asset::Column::Owner.eq(x)))
            .add_option(
                self.owner_type
                    .clone()
                    .map(|x| asset::Column::OwnerType.eq(x)),
            )
            .add_option(validate_opt_pubkey(&self.delegate)?.map(|x| asset::Column::Delegate.eq(x)))
            .add_option(self.frozen.map(|x| asset::Column::Frozen.eq(x)))
            .add_option(self.supply.map(|x| asset::Column::Supply.eq(x)))
            .add_option(
                validate_opt_pubkey(&self.supply_mint)?.map(|x| asset::Column::SupplyMint.eq(x)),
            )
            .add_option(self.compressed.map(|x| asset::Column::Compressed.eq(x)))
            .add_option(self.compressible.map(|x| asset::Column::Compressible.eq(x)))
            .add_option(self.seq.map(|x| asset::Column::Seq.eq(x)))
            .add_option(validate_opt_pubkey(&self.tree_id)?.map(|x| asset::Column::TreeId.eq(x)))
            .add_option(validate_opt_pubkey(&self.leaf)?.map(|x| asset::Column::Leaf.eq(x)))
            .add_option(self.nonce.map(|x| asset::Column::Nonce.eq(x)))
            .add_option(
                self.royalty_target_type
                    .clone()
                    .map(|x| asset::Column::RoyaltyTargetType.eq(x)),
            )
            .add_option(
                validate_opt_pubkey(&self.royalty_target)?
                    .map(|x| asset::Column::RoyaltyTarget.eq(x)),
            )
            .add_option(
                self.royalty_amount
                    .map(|x| asset::Column::RoyaltyAmount.eq(x)),
            )
            .add_option(
                validate_opt_pubkey(&self.asset_data)?.map(|x| asset::Column::AssetData.eq(x)),
            )
            .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)))
            .add_option(self.slot_updated.map(|x| asset::Column::SlotUpdated.eq(x)));

        match self.negate {
            None | Some(false) => Ok(conditions),
            Some(true) => Ok(conditions.not()),
        }
    }
}

fn validate_opt_pubkey(pubkey: &Option<String>) -> Result<Option<Vec<u8>>, DbErr> {
    let opt_bytes = if let Some(pubkey) = pubkey {
        let pubkey = Pubkey::from_str(pubkey)
            .map_err(|_| DbErr::Custom(format!("Invalid pubkey {}", pubkey)))?;
        Some(pubkey.to_bytes().to_vec())
    } else {
        None
    };
    Ok(opt_bytes)
}
