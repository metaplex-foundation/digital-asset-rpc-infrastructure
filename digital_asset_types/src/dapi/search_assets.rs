use crate::{
    dao::{
        asset,
        sea_orm_active_enums::{
            OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
        }, scopes,
    },
    rpc::{filter::AssetSorting, response::AssetList},
};
use sea_orm::{entity::*, query::*, DatabaseConnection, DbErr};
use serde::{Deserialize,Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use super::common::{create_pagination, create_sorting, build_asset_response};



pub async fn search_assets(
    db: &DatabaseConnection,
    search_assets_query: SearchAssetsQuery,
    sorting: AssetSorting,
    limit: u64,
    page: Option<u64>,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(before, after, page)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    let condition = search_assets_query.conditions()?;
    let assets = scopes::asset::get_assets_by_condition(
        db,
        condition,
        sort_column,
        sort_direction,
        &pagination,
        limit,
    )
    .await?;
    Ok(build_asset_response(assets, limit, &pagination))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchAssetsQuery {
    // Conditions
    negate: Option<bool>,
    /// Defaults to [ConditionType::All]
    condition_type: Option<ConditionType>,
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
    royalty_target_type: Option<RoyaltyTargetType>,
    royalty_target: Option<String>,
    royalty_amount: Option<u32>,
    burnt: Option<bool>
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum ConditionType {
    Any,
    All,
}

impl SearchAssetsQuery {
    pub fn count_conditions(&self) -> usize {
        // Initialize counter
        let mut num_conditions = 0;
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
        if self.royalty_target_type.is_some() {
            num_conditions += 1;
        }
        if self.royalty_target.is_some() {
            num_conditions += 1;
        }
        if self.royalty_amount.is_some() {
            num_conditions += 1;
        }
        if self.burnt.is_some() {
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
            .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)));

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
