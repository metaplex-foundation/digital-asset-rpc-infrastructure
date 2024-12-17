#![allow(ambiguous_glob_reexports)]
mod full_asset;
mod generated;
pub mod scopes;
use crate::rpc::filter::TokenTypeClass;

use self::sea_orm_active_enums::{
    OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
};
pub use full_asset::*;
pub use generated::*;
pub mod extensions;

use sea_orm::{
    entity::*,
    sea_query::Expr,
    sea_query::{ConditionType, IntoCondition, SimpleExpr},
    Condition, DbErr, RelationDef,
};
use serde::{Deserialize, Serialize};

pub struct GroupingSize {
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PageOptions {
    pub limit: u64,
    pub page: Option<u64>,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub cursor: Option<Cursor>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct Cursor {
    pub id: Option<Vec<u8>>,
}

pub enum Pagination {
    Keyset {
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
    },
    Page {
        page: u64,
    },
    Cursor(Cursor),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchAssetsQuery {
    // Conditions
    pub negate: Option<bool>,
    /// Defaults to [ConditionType::All]
    pub condition_type: Option<ConditionType>,
    pub specification_version: Option<SpecificationVersions>,
    pub token_type: Option<TokenTypeClass>,
    pub owner_address: Option<Vec<u8>>,
    pub owner_type: Option<OwnerType>,
    pub creator_address: Option<Vec<u8>>,
    pub creator_verified: Option<bool>,
    pub authority_address: Option<Vec<u8>>,
    pub grouping: Option<(String, String)>,
    pub delegate: Option<Vec<u8>>,
    pub frozen: Option<bool>,
    pub supply: Option<u64>,
    pub supply_mint: Option<Vec<u8>>,
    pub compressed: Option<bool>,
    pub compressible: Option<bool>,
    pub royalty_target_type: Option<RoyaltyTargetType>,
    pub royalty_target: Option<Vec<u8>>,
    pub royalty_amount: Option<u32>,
    pub burnt: Option<bool>,
    pub json_uri: Option<String>,
    pub name: Option<Vec<u8>>,
}

impl SearchAssetsQuery {
    pub fn conditions(&self) -> Result<(Condition, Vec<RelationDef>), DbErr> {
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
            .add_option(self.token_type.clone().map(|x| {
                match x {
                    TokenTypeClass::Compressed => asset::Column::TreeId.is_not_null(),
                    TokenTypeClass::Nft => {
                        asset::Column::TreeId.is_null()
                        .and(
                            asset::Column::SpecificationAssetClass.eq(SpecificationAssetClass::Nft)
                            .or(asset::Column::SpecificationAssetClass.eq(SpecificationAssetClass::MplCoreAsset))
                            .or(asset::Column::SpecificationAssetClass.eq(SpecificationAssetClass::ProgrammableNft))
                        )
                    },
                    TokenTypeClass::NonFungible => {
                    asset::Column::SpecificationAssetClass.eq(SpecificationAssetClass::Nft)
                    .or(asset::Column::SpecificationAssetClass
                        .eq(SpecificationAssetClass::ProgrammableNft))
                        .or(asset::Column::SpecificationAssetClass.eq(SpecificationAssetClass::MplCoreAsset))
                        },
                    TokenTypeClass::Fungible => asset::Column::SpecificationAssetClass
                        .eq(SpecificationAssetClass::FungibleAsset)
                        .or(asset::Column::SpecificationAssetClass
                            .eq(SpecificationAssetClass::FungibleToken)),
                    TokenTypeClass::All => asset::Column::SpecificationAssetClass.is_not_null(),
                }
            }))
            .add_option(
                self.owner_address
                    .to_owned()
                    .map(|x| asset::Column::Owner.eq(x)),
            )
            .add_option(
                self.delegate
                    .to_owned()
                    .map(|x| asset::Column::Delegate.eq(x)),
            )
            .add_option(self.frozen.map(|x| asset::Column::Frozen.eq(x)))
            .add_option(
                self.supply_mint
                    .to_owned()
                    .map(|x| asset::Column::SupplyMint.eq(x)),
            )
            .add_option(self.compressed.map(|x| asset::Column::Compressed.eq(x)))
            .add_option(self.compressible.map(|x| asset::Column::Compressible.eq(x)))
            .add_option(
                self.royalty_target_type
                    .clone()
                    .map(|x| asset::Column::RoyaltyTargetType.eq(x)),
            )
            .add_option(
                self.royalty_target
                    .to_owned()
                    .map(|x| asset::Column::RoyaltyTarget.eq(x)),
            )
            .add_option(
                self.royalty_amount
                    .map(|x| asset::Column::RoyaltyAmount.eq(x)),
            )
            .add_option(self.burnt.map(|x| asset::Column::Burnt.eq(x)));

        if let Some(s) = self.supply {
            conditions = conditions.add(asset::Column::Supply.eq(s));
        } else {
            // By default, we ignore malformed tokens by ignoring tokens with supply=0
            // unless they are burnt.
            //
            // cNFTs keep supply=1 after they are burnt.
            // Regular NFTs go to supply=0 after they are burnt.
            conditions = conditions.add(
                asset::Column::Supply
                    .ne(0)
                    .or(asset::Column::Burnt.eq(true)),
            )
        }

        if let Some(o) = self.owner_type.clone() {
            conditions = conditions.add(asset::Column::OwnerType.eq(o));
        } else {
            // Default to NFTs
            //
            // In theory, the owner_type=single check should be sufficient,
            // however there is an old bug that has marked some non-NFTs as "single" with supply > 1.
            // The supply check guarentees we do not include those.
            conditions = conditions.add_option(Some(
                asset::Column::OwnerType
                    .eq(OwnerType::Single)
                    .and(asset::Column::Supply.lte(1)),
            ));
        }

        if let Some(c) = self.creator_address.to_owned() {
            conditions = conditions.add(asset_creators::Column::Creator.eq(c));
        }

        // Without specifying the creators themselves, there is no index being hit.
        // So in some rare scenarios, this query could be very slow.
        if let Some(cv) = self.creator_verified.to_owned() {
            conditions = conditions.add(asset_creators::Column::Verified.eq(cv));
        }

        // If creator_address or creator_verified is set, join with asset_creators
        let mut joins = Vec::new();
        if self.creator_address.is_some() || self.creator_verified.is_some() {
            let rel = extensions::asset_creators::Relation::Asset
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::tbl(right, asset_creators::Column::AssetId)
                        .eq(Expr::tbl(left, asset::Column::Id))
                        .into_condition()
                });
            joins.push(rel);
        }

        if let Some(a) = self.authority_address.to_owned() {
            conditions = conditions.add(asset_authority::Column::Authority.eq(a));
            let rel = extensions::asset_authority::Relation::Asset
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::tbl(right, asset_authority::Column::AssetId)
                        .eq(Expr::tbl(left, asset::Column::Id))
                        .into_condition()
                });
            joins.push(rel);
        }

        if let Some(g) = self.grouping.to_owned() {
            let cond = Condition::all()
                .add(asset_grouping::Column::GroupKey.eq(g.0))
                .add(asset_grouping::Column::GroupValue.eq(g.1));
            conditions = conditions.add(cond);
            let rel = extensions::asset_grouping::Relation::Asset
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::tbl(right, asset_grouping::Column::AssetId)
                        .eq(Expr::tbl(left, asset::Column::Id))
                        .into_condition()
                });
            joins.push(rel);
        }

        if let Some(ju) = self.json_uri.to_owned() {
            let cond = Condition::all().add(asset_data::Column::MetadataUrl.eq(ju));
            conditions = conditions.add(cond);
            let rel = extensions::asset_data::Relation::Asset
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::tbl(right, asset_data::Column::Id)
                        .eq(Expr::tbl(left, asset::Column::AssetData))
                        .into_condition()
                });
            joins.push(rel);
        }

        if let Some(n) = self.name.to_owned() {
            let name_as_str = std::str::from_utf8(&n).map_err(|_| {
                DbErr::Custom(
                    "Could not convert raw name bytes into string for comparison".to_owned(),
                )
            })?;

            let name_expr =
                SimpleExpr::Custom(format!("chain_data->>'name' LIKE '%{}%'", name_as_str));

            conditions = conditions.add(name_expr);
            let rel = extensions::asset_data::Relation::Asset
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::tbl(right, asset_data::Column::Id)
                        .eq(Expr::tbl(left, asset::Column::AssetData))
                        .into_condition()
                });
            joins.push(rel);
        }

        Ok((
            match self.negate {
                None | Some(false) => conditions,
                Some(true) => conditions.not(),
            },
            joins,
        ))
    }
}
