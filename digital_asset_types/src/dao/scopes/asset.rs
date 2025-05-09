use crate::{
    dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping,
        asset_v1_account_attachments, cl_audits_v2,
        extensions::{self, instruction::PascalCase},
        generated::sea_orm_active_enums::OwnerType,
        sea_orm_active_enums::{Instruction, V1AccountAttachments},
        token_accounts, tokens, Cursor, FullAsset, GroupingSize, Pagination,
    },
    rpc::{
        filter::AssetSortDirection,
        options::Options,
        response::{NftEdition, NftEditions},
    },
};
use indexmap::IndexMap;
use mpl_token_metadata::accounts::{Edition, MasterEdition};
use sea_orm::{
    prelude::Decimal,
    sea_query::{Alias, Condition, Expr, PostgresQueryBuilder, Query, UnionType},
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, FromQueryResult, JoinType, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationDef, RelationTrait, Statement,
};
use serde::de::DeserializeOwned;
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, hash::RandomState};

pub fn paginate<T, C>(
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

#[allow(clippy::too_many_arguments)]
pub async fn get_by_creator(
    conn: &impl ConnectionTrait,
    creator: Vec<u8>,
    only_verified: bool,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = Condition::all()
        .add(asset_creators::Column::Creator.eq(creator.clone()))
        .add(asset::Column::Supply.gt(0));
    if only_verified {
        condition = condition.add(asset_creators::Column::Verified.eq(true));
    }

    if !options.show_fungible {
        condition = condition.add(asset::Column::OwnerType.eq(OwnerType::Single));
    }

    get_by_related_condition(
        conn,
        condition,
        extensions::asset::Relation::AssetCreators,
        None,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
        Some(creator),
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

#[allow(clippy::too_many_arguments)]
pub async fn get_by_grouping(
    conn: &impl ConnectionTrait,
    group_key: String,
    group_value: String,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut condition = Condition::all().add(
        asset_grouping::Column::GroupKey
            .eq(group_key)
            .and(asset_grouping::Column::GroupValue.eq(group_value)),
    );

    if !options.show_unverified_collections {
        condition = condition.add(
            asset_grouping::Column::Verified
                .eq(true)
                .or(asset_grouping::Column::Verified.is_null()),
        );
    }

    if !options.show_fungible {
        condition = condition.add(asset::Column::OwnerType.eq(OwnerType::Single));
    }

    get_by_related_condition(
        conn,
        Condition::all()
            .add(condition)
            .add(asset::Column::Supply.gt(0)),
        extensions::asset::Relation::AssetGrouping,
        None,
        sort_by,
        sort_direction,
        pagination,
        limit,
        options,
        None,
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
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let token_owner_query = Query::select()
        .column((asset::Entity, asset::Column::Id))
        .column((asset::Entity, asset::Column::AltId))
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationVersion))
                .as_enum(Alias::new("TEXT")),
        )
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationAssetClass))
                .as_enum(Alias::new("TEXT")),
        )
        .column((token_accounts::Entity, token_accounts::Column::Owner))
        .expr(Expr::col((asset::Entity, asset::Column::OwnerType)).as_enum(Alias::new("TEXT")))
        .column((token_accounts::Entity, token_accounts::Column::Delegate))
        .column((token_accounts::Entity, token_accounts::Column::Frozen))
        .column((asset::Entity, asset::Column::Supply))
        .column((asset::Entity, asset::Column::SupplyMint))
        .column((asset::Entity, asset::Column::Compressed))
        .column((asset::Entity, asset::Column::Compressible))
        .column((asset::Entity, asset::Column::Seq))
        .column((asset::Entity, asset::Column::TreeId))
        .column((asset::Entity, asset::Column::Leaf))
        .column((asset::Entity, asset::Column::Nonce))
        .expr(
            Expr::col((asset::Entity, asset::Column::RoyaltyTargetType))
                .as_enum(Alias::new("TEXT")),
        )
        .column((asset::Entity, asset::Column::RoyaltyTarget))
        .column((asset::Entity, asset::Column::RoyaltyAmount))
        .column((asset::Entity, asset::Column::AssetData))
        .column((asset::Entity, asset::Column::CreatedAt))
        .column((asset::Entity, asset::Column::Burnt))
        .column((asset::Entity, asset::Column::SlotUpdated))
        .column((asset::Entity, asset::Column::SlotUpdatedMetadataAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedMintAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedTokenAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedCnftTransaction))
        .column((asset::Entity, asset::Column::DataHash))
        .column((asset::Entity, asset::Column::CreatorHash))
        .column((asset::Entity, asset::Column::OwnerDelegateSeq))
        .column((asset::Entity, asset::Column::LeafSeq))
        .column((asset::Entity, asset::Column::BaseInfoSeq))
        .column((asset::Entity, asset::Column::MintExtensions))
        .column((asset::Entity, asset::Column::MplCorePlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownPlugins))
        .column((asset::Entity, asset::Column::MplCoreCollectionNumMinted))
        .column((asset::Entity, asset::Column::MplCoreCollectionCurrentSize))
        .column((asset::Entity, asset::Column::MplCorePluginsJsonVersion))
        .column((asset::Entity, asset::Column::MplCoreExternalPlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownExternalPlugins))
        .column((asset::Entity, asset::Column::CollectionHash))
        .column((asset::Entity, asset::Column::AssetDataHash))
        .column((asset::Entity, asset::Column::BubblegumFlags))
        .column((asset::Entity, asset::Column::NonTransferable))
        .from(asset::Entity)
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(token_accounts::Entity, token_accounts::Column::Mint),
        )
        .and_where(token_accounts::Column::Owner.eq(owner.to_vec()))
        .and_where(token_accounts::Column::Amount.gt(0))
        .cond_where(
            Condition::any()
                .add(asset::Column::Owner.ne(owner.to_vec()))
                .add(asset::Column::Owner.is_null()),
        )
        .to_owned();

    let mut stmt = Query::select()
        .column((asset::Entity, asset::Column::Id))
        .column((asset::Entity, asset::Column::AltId))
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationVersion))
                .as_enum(Alias::new("TEXT")),
        )
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationAssetClass))
                .as_enum(Alias::new("TEXT")),
        )
        .column((asset::Entity, asset::Column::Owner))
        .expr(Expr::col((asset::Entity, asset::Column::OwnerType)).as_enum(Alias::new("TEXT")))
        .column((asset::Entity, asset::Column::Delegate))
        .column((asset::Entity, asset::Column::Frozen))
        .column((asset::Entity, asset::Column::Supply))
        .column((asset::Entity, asset::Column::SupplyMint))
        .column((asset::Entity, asset::Column::Compressed))
        .column((asset::Entity, asset::Column::Compressible))
        .column((asset::Entity, asset::Column::Seq))
        .column((asset::Entity, asset::Column::TreeId))
        .column((asset::Entity, asset::Column::Leaf))
        .column((asset::Entity, asset::Column::Nonce))
        .expr(
            Expr::col((asset::Entity, asset::Column::RoyaltyTargetType))
                .as_enum(Alias::new("TEXT")),
        )
        .column((asset::Entity, asset::Column::RoyaltyTarget))
        .column((asset::Entity, asset::Column::RoyaltyAmount))
        .column((asset::Entity, asset::Column::AssetData))
        .column((asset::Entity, asset::Column::CreatedAt))
        .column((asset::Entity, asset::Column::Burnt))
        .column((asset::Entity, asset::Column::SlotUpdated))
        .column((asset::Entity, asset::Column::SlotUpdatedMetadataAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedMintAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedTokenAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedCnftTransaction))
        .column((asset::Entity, asset::Column::DataHash))
        .column((asset::Entity, asset::Column::CreatorHash))
        .column((asset::Entity, asset::Column::OwnerDelegateSeq))
        .column((asset::Entity, asset::Column::LeafSeq))
        .column((asset::Entity, asset::Column::BaseInfoSeq))
        .column((asset::Entity, asset::Column::MintExtensions))
        .column((asset::Entity, asset::Column::MplCorePlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownPlugins))
        .column((asset::Entity, asset::Column::MplCoreCollectionNumMinted))
        .column((asset::Entity, asset::Column::MplCoreCollectionCurrentSize))
        .column((asset::Entity, asset::Column::MplCorePluginsJsonVersion))
        .column((asset::Entity, asset::Column::MplCoreExternalPlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownExternalPlugins))
        .column((asset::Entity, asset::Column::CollectionHash))
        .column((asset::Entity, asset::Column::AssetDataHash))
        .column((asset::Entity, asset::Column::BubblegumFlags))
        .column((asset::Entity, asset::Column::NonTransferable))
        .from(asset::Entity)
        .and_where(asset::Column::Owner.eq(owner.to_vec()))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if options.show_fungible {
        stmt = stmt.union(UnionType::All, token_owner_query).to_owned()
    }

    if let Some(col) = sort_by {
        stmt = stmt
            .order_by(col, sort_direction.clone())
            .order_by(asset::Column::Id, sort_direction.clone())
            .to_owned();
    }

    match pagination {
        Pagination::Keyset { before, after } => {
            if let Some(b) = before {
                stmt = stmt.and_where(asset::Column::Id.lt(b.clone())).to_owned();
            }
            if let Some(a) = after {
                stmt = stmt.and_where(asset::Column::Id.gt(a.clone())).to_owned();
            }
        }
        Pagination::Page { page } => {
            if *page > 0 {
                stmt = stmt.offset((page - 1) * limit).to_owned();
            }
        }
        Pagination::Cursor(cursor) => {
            if *cursor != Cursor::default() {
                if sort_direction == sea_orm::Order::Asc {
                    stmt = stmt
                        .and_where(asset::Column::Id.gt(cursor.id.clone()))
                        .to_owned();
                } else {
                    stmt = stmt
                        .and_where(asset::Column::Id.lt(cursor.id.clone()))
                        .to_owned();
                }
            }
        }
    }
    stmt = stmt.limit(limit).to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = asset::Model::find_by_statement(statment).all(conn).await?;

    get_related_for_assets(conn, assets, Some(owner), options, None).await
}

pub async fn get_assets(
    conn: &impl ConnectionTrait,
    asset_ids: Vec<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let cond = Condition::all()
        .add(asset::Column::Id.is_in(asset_ids))
        .add(asset::Column::Supply.gt(0));

    get_assets_by_condition(
        conn,
        cond,
        vec![],
        None,
        None,
        Order::Asc,
        pagination,
        limit,
        options,
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
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr> {
    let mut stmt = Query::select()
        .column((asset::Entity, asset::Column::Id))
        .column((asset::Entity, asset::Column::AltId))
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationVersion))
                .as_enum(Alias::new("TEXT")),
        )
        .expr(
            Expr::col((asset::Entity, asset::Column::SpecificationAssetClass))
                .as_enum(Alias::new("TEXT")),
        )
        .column((asset::Entity, asset::Column::Owner))
        .expr(Expr::col((asset::Entity, asset::Column::OwnerType)).as_enum(Alias::new("TEXT")))
        .column((asset::Entity, asset::Column::Delegate))
        .column((asset::Entity, asset::Column::Frozen))
        .column((asset::Entity, asset::Column::Supply))
        .column((asset::Entity, asset::Column::SupplyMint))
        .column((asset::Entity, asset::Column::Compressed))
        .column((asset::Entity, asset::Column::Compressible))
        .column((asset::Entity, asset::Column::Seq))
        .column((asset::Entity, asset::Column::TreeId))
        .column((asset::Entity, asset::Column::Leaf))
        .column((asset::Entity, asset::Column::Nonce))
        .expr(
            Expr::col((asset::Entity, asset::Column::RoyaltyTargetType))
                .as_enum(Alias::new("TEXT")),
        )
        .column((asset::Entity, asset::Column::RoyaltyTarget))
        .column((asset::Entity, asset::Column::RoyaltyAmount))
        .column((asset::Entity, asset::Column::AssetData))
        .column((asset::Entity, asset::Column::CreatedAt))
        .column((asset::Entity, asset::Column::Burnt))
        .column((asset::Entity, asset::Column::SlotUpdated))
        .column((asset::Entity, asset::Column::SlotUpdatedMetadataAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedMintAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedTokenAccount))
        .column((asset::Entity, asset::Column::SlotUpdatedCnftTransaction))
        .column((asset::Entity, asset::Column::DataHash))
        .column((asset::Entity, asset::Column::CreatorHash))
        .column((asset::Entity, asset::Column::OwnerDelegateSeq))
        .column((asset::Entity, asset::Column::LeafSeq))
        .column((asset::Entity, asset::Column::BaseInfoSeq))
        .column((asset::Entity, asset::Column::MintExtensions))
        .column((asset::Entity, asset::Column::MplCorePlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownPlugins))
        .column((asset::Entity, asset::Column::MplCoreCollectionNumMinted))
        .column((asset::Entity, asset::Column::MplCoreCollectionCurrentSize))
        .column((asset::Entity, asset::Column::MplCorePluginsJsonVersion))
        .column((asset::Entity, asset::Column::MplCoreExternalPlugins))
        .column((asset::Entity, asset::Column::MplCoreUnknownExternalPlugins))
        .column((asset::Entity, asset::Column::CollectionHash))
        .column((asset::Entity, asset::Column::AssetDataHash))
        .column((asset::Entity, asset::Column::BubblegumFlags))
        .column((asset::Entity, asset::Column::NonTransferable))
        .from(asset::Entity)
        .join(
            JoinType::LeftJoin,
            asset_authority::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(asset_authority::Entity, asset_authority::Column::AssetId),
        )
        .and_where(asset_authority::Column::Authority.eq(authority))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if !options.show_fungible {
        stmt = stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    if let Some(col) = sort_by {
        stmt = stmt
            .order_by(col, sort_direction.clone())
            .order_by(asset::Column::Id, sort_direction.clone())
            .to_owned();
    }

    match pagination {
        Pagination::Keyset { before, after } => {
            if let Some(b) = before {
                stmt = stmt.and_where(asset::Column::Id.lt(b.clone())).to_owned();
            }
            if let Some(a) = after {
                stmt = stmt.and_where(asset::Column::Id.gt(a.clone())).to_owned();
            }
        }
        Pagination::Page { page } => {
            if *page > 0 {
                stmt = stmt.offset((page - 1) * limit).to_owned();
            }
        }
        Pagination::Cursor(cursor) => {
            if *cursor != Cursor::default() {
                if sort_direction == sea_orm::Order::Asc {
                    stmt = stmt
                        .and_where(asset::Column::Id.gt(cursor.id.clone()))
                        .to_owned();
                } else {
                    stmt = stmt
                        .and_where(asset::Column::Id.lt(cursor.id.clone()))
                        .to_owned();
                }
            }
        }
    }
    stmt = stmt.limit(limit).to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = asset::Model::find_by_statement(statment).all(conn).await?;

    get_related_for_assets(conn, assets, None, options, None).await
}

#[allow(clippy::too_many_arguments)]
async fn get_by_related_condition<E>(
    conn: &impl ConnectionTrait,
    condition: Condition,
    relation: E,
    owner: Option<Vec<u8>>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
    required_creator: Option<Vec<u8>>,
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

    get_related_for_assets(conn, assets, owner, options, required_creator).await
}

pub async fn get_related_for_assets(
    conn: &impl ConnectionTrait,
    assets: Vec<asset::Model>,
    owner: Option<Vec<u8>>,
    options: &Options,
    required_creator: Option<Vec<u8>>,
) -> Result<Vec<FullAsset>, DbErr> {
    let asset_ids = assets.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    let asset_data: Vec<asset_data::Model> = asset_data::Entity::find()
        .filter(asset_data::Column::Id.is_in(asset_ids.clone()))
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
                ..Default::default()
            };
            acc.insert(id, fa);
        };
        acc
    });

    let ids = assets_map.keys().cloned().collect::<Vec<_>>();

    // Get all creators for all assets in `assets_map``.
    let creators = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.is_in(ids))
        .order_by_asc(asset_creators::Column::AssetId)
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;

    // Add the creators to the assets in `asset_map``.
    for c in creators.into_iter() {
        if let Some(asset) = assets_map.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    // Filter out stale creators from each asset.
    for (_id, asset) in assets_map.iter_mut() {
        filter_out_stale_creators(&mut asset.creators);
    }

    // If we passed in a required creator, we make sure that creator is still in the creator array
    // of each asset after stale creators were filtered out above.  Only retain those assets that
    // have the required creator.  This corrects `getAssetByCreators` from returning assets for
    // which the required creator is no longer in the creator array.
    if let Some(required) = required_creator {
        assets_map.retain(|_id, asset| asset.creators.iter().any(|c| c.creator == required));
    }

    let ids = assets_map.keys().cloned().collect::<Vec<_>>();
    let authorities = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.is_in(ids.clone()))
        .all(conn)
        .await?;
    for a in authorities.into_iter() {
        if let Some(asset) = assets_map.get_mut(&a.asset_id) {
            asset.authorities.push(a);
        }
    }

    let mut token_account_condition = Condition::all();

    let mut asset_owners: Vec<Vec<u8>> = Vec::new();

    for id in asset_ids.iter() {
        if let Some(asset) = assets_map.get(id) {
            if let Some(owner) = asset.asset.owner.clone() {
                asset_owners.push(owner);
            }
        }
    }

    if let Some(query_owner) = owner {
        token_account_condition =
            token_account_condition.add(token_accounts::Column::Owner.eq(query_owner));
    } else {
        token_account_condition =
            token_account_condition.add(token_accounts::Column::Owner.is_in(asset_owners));
    };

    let token_accounts = tokens::Entity::find()
        .find_also_related(token_accounts::Entity)
        .filter(tokens::Column::Mint.is_in(asset_ids.clone()))
        .filter(token_account_condition)
        .filter(token_accounts::Column::Amount.gt(0))
        .all(conn)
        .await?;

    for (t, ta) in token_accounts.into_iter() {
        if let Some(asset) = assets_map.get_mut(&t.mint) {
            if let Some(ta) = ta {
                asset.asset.owner = Some(ta.owner.clone());
                asset.token_account = Some(ta);
            }
            asset.mint = Some(t.clone());
        }
    }

    let cond = if options.show_unverified_collections {
        Condition::all()
    } else {
        Condition::any()
            .add(asset_grouping::Column::Verified.eq(true))
            // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
            // Therefore if verified is null, we can assume that the group is verified.
            .add(asset_grouping::Column::Verified.is_null())
    };

    let grouping_base_query = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.is_in(ids.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(cond);

    if options.show_inscription {
        let attachments = asset_v1_account_attachments::Entity::find()
            .filter(asset_v1_account_attachments::Column::AssetId.is_in(asset_ids))
            .all(conn)
            .await?;

        for a in attachments.into_iter() {
            if let Some(asset) = assets_map.get_mut(&a.id) {
                asset.inscription = Some(a);
            }
        }
    }

    if options.show_collection_metadata {
        let groups = grouping_base_query.all(conn).await?;

        let group_values = groups
            .iter()
            .filter_map(|group| {
                group
                    .group_value
                    .as_ref()
                    .and_then(|g| bs58::decode(g).into_vec().ok())
            })
            .collect::<Vec<_>>();

        let asset_data = asset_data::Entity::find()
            .filter(asset_data::Column::Id.is_in(group_values))
            .all(conn)
            .await?;

        let asset_data_map: HashMap<_, _, RandomState> = HashMap::from_iter(
            asset_data
                .into_iter()
                .map(|ad| (ad.id.clone(), ad))
                .collect::<Vec<_>>(),
        );

        for g in groups.into_iter() {
            if let Some(asset) = assets_map.get_mut(&g.asset_id) {
                let a = g.group_value.as_ref().and_then(|g| {
                    bs58::decode(g)
                        .into_vec()
                        .ok()
                        .and_then(|v| asset_data_map.get(&v))
                        .cloned()
                });

                asset.groups.push((g, a));
            }
        }
    } else {
        let single_group_query = grouping_base_query.all(conn).await?;
        for g in single_group_query.into_iter() {
            if let Some(asset) = assets_map.get_mut(&g.asset_id) {
                asset.groups.push((g, None));
            }
        }
    };

    Ok(assets_map.into_iter().map(|(_, v)| v).collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn get_assets_by_condition(
    conn: &impl ConnectionTrait,
    condition: Condition,
    joins: Vec<RelationDef>,
    owner: Option<Vec<u8>>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
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

    let full_assets = get_related_for_assets(conn, assets, owner, options, None).await?;
    Ok(full_assets)
}

pub async fn get_by_id(
    conn: &impl ConnectionTrait,
    asset_id: Vec<u8>,
    options: &Options,
) -> Result<FullAsset, DbErr> {
    let asset_data =
        asset::Entity::find_by_id(asset_id.clone()).find_also_related(asset_data::Entity);

    let inscription = if options.show_inscription {
        get_inscription_by_mint(conn, asset_id.clone()).await.ok()
    } else {
        None
    };

    let mint = tokens::Entity::find()
        .filter(tokens::Column::Mint.eq(asset_id.clone()))
        .filter(tokens::Column::Supply.gt(0))
        .one(conn)
        .await?;

    let (asset, data): (asset::Model, asset_data::Model) =
        asset_data.one(conn).await.and_then(|o| match o {
            Some((a, Some(d))) => Ok((a, d)),
            _ => Err(DbErr::RecordNotFound("Asset Not Found".to_string())),
        })?;

    if asset.supply == Decimal::from(0) {
        return Err(DbErr::Custom("Asset has no supply".to_string()));
    }

    let authorities: Vec<asset_authority::Model> = asset_authority::Entity::find()
        .filter(asset_authority::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_authority::Column::AssetId)
        .all(conn)
        .await?;

    let mut creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
        .filter(asset_creators::Column::AssetId.eq(asset.id.clone()))
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;

    filter_out_stale_creators(&mut creators);

    let grouping_query = asset_grouping::Entity::find()
        .filter(asset_grouping::Column::AssetId.eq(asset.id.clone()))
        .filter(asset_grouping::Column::GroupValue.is_not_null())
        .filter(
            Condition::any()
                .add(asset_grouping::Column::Verified.eq(true))
                // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
                // Therefore if verified is null, we can assume that the group is verified.
                .add(asset_grouping::Column::Verified.is_null()),
        )
        .order_by_asc(asset_grouping::Column::AssetId);

    let groups = if options.show_collection_metadata {
        let groups = grouping_query.all(conn).await?;

        let group_values = groups
            .iter()
            .filter_map(|group| {
                group
                    .group_value
                    .as_ref()
                    .and_then(|g| bs58::decode(g).into_vec().ok())
            })
            .collect::<Vec<_>>();

        let asset_data = asset_data::Entity::find()
            .filter(asset_data::Column::Id.is_in(group_values))
            .all(conn)
            .await?;

        let asset_data_map: HashMap<_, _, RandomState> = HashMap::from_iter(
            asset_data
                .into_iter()
                .map(|ad| (ad.id.clone(), ad))
                .collect::<Vec<_>>(),
        );

        let mut groups_tup = Vec::new();
        for g in groups.into_iter() {
            let a = g.group_value.as_ref().and_then(|g| {
                bs58::decode(g)
                    .into_vec()
                    .ok()
                    .and_then(|v| asset_data_map.get(&v))
                    .cloned()
            });

            groups_tup.push((g, a));
        }

        groups_tup
    } else {
        grouping_query
            .all(conn)
            .await?
            .into_iter()
            .map(|g| (g, None))
            .collect::<Vec<_>>()
    };

    Ok(FullAsset {
        asset,
        data,
        authorities,
        creators,
        inscription,
        groups,
        mint,
        ..Default::default()
    })
}

pub async fn fetch_transactions(
    conn: &impl ConnectionTrait,
    tree: Vec<u8>,
    leaf_idx: i64,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // Default sort direction is Desc
    // Similar to GetSignaturesForAddress in the Solana API
    let sort_direction = sort_direction.unwrap_or(AssetSortDirection::Desc);
    let sort_order = match sort_direction {
        AssetSortDirection::Asc => sea_orm::Order::Asc,
        AssetSortDirection::Desc => sea_orm::Order::Desc,
    };

    let mut stmt = cl_audits_v2::Entity::find().filter(cl_audits_v2::Column::Tree.eq(tree));
    stmt = stmt.filter(cl_audits_v2::Column::LeafIdx.eq(leaf_idx));
    stmt = stmt.order_by(cl_audits_v2::Column::Seq, sort_order.clone());

    stmt = paginate(
        pagination,
        limit,
        stmt,
        sort_order,
        cl_audits_v2::Column::Seq,
    );
    let transactions = stmt.all(conn).await?;
    let transaction_list = transactions
        .into_iter()
        .map(|transaction| {
            let tx = bs58::encode(transaction.tx).into_string();
            let ix = Instruction::to_pascal_case(&transaction.instruction).to_string();
            (tx, ix)
        })
        .collect();

    Ok(transaction_list)
}

pub async fn get_asset_signatures(
    conn: &impl ConnectionTrait,
    asset_id: Option<Vec<u8>>,
    tree_id: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // if tree_id and leaf_idx are provided, use them directly to fetch transactions
    if let (Some(tree_id), Some(leaf_idx)) = (tree_id, leaf_idx) {
        let transactions =
            fetch_transactions(conn, tree_id, leaf_idx, pagination, limit, sort_direction).await?;
        return Ok(transactions);
    }

    if asset_id.is_none() {
        return Err(DbErr::Custom(
            "Either 'id' or both 'tree' and 'leafIndex' must be provided".to_string(),
        ));
    }

    // if only asset_id is provided, fetch the latest tree and leaf_idx (asset.nonce) for the asset
    // and use them to fetch transactions
    let stmt = asset::Entity::find()
        .distinct_on([(asset::Entity, asset::Column::Id)])
        .filter(asset::Column::Id.eq(asset_id))
        .order_by(asset::Column::Id, Order::Desc)
        .limit(1);
    let asset = stmt.one(conn).await?;
    if let Some(asset) = asset {
        let tree = asset
            .tree_id
            .ok_or(DbErr::RecordNotFound("Tree not found".to_string()))?;
        if tree.is_empty() {
            return Err(DbErr::Custom("Empty tree for asset".to_string()));
        }
        let leaf_idx = asset
            .nonce
            .ok_or(DbErr::RecordNotFound("Leaf ID does not exist".to_string()))?;
        let transactions =
            fetch_transactions(conn, tree, leaf_idx, pagination, limit, sort_direction).await?;
        Ok(transactions)
    } else {
        Ok(Vec::new())
    }
}

fn filter_out_stale_creators(creators: &mut Vec<asset_creators::Model>) {
    // If the first creator is an empty Vec, it means the creator array is empty (which is allowed
    // for compressed assets in Bubblegum).
    if !creators.is_empty() && creators[0].creator.is_empty() {
        creators.clear();
    } else {
        // For both compressed and non-compressed assets, any creators that do not have the max
        // `slot_updated` value are stale and should be removed.
        let max_slot_updated = creators.iter().map(|creator| creator.slot_updated).max();
        if let Some(max_slot_updated) = max_slot_updated {
            creators.retain(|creator| creator.slot_updated == max_slot_updated);
        }

        // For compressed assets, any creators that do not have the max `seq` value are stale and
        // should be removed.  A `seq` value of 0 indicates a decompressed or never-compressed
        // asset.  So if a `seq` value of 0 is present, then all creators with nonzero `seq` values
        // are stale and should be removed.
        let seq = if creators
            .iter()
            .map(|creator| creator.seq)
            .any(|seq| seq == Some(0))
        {
            Some(Some(0))
        } else {
            creators.iter().map(|creator| creator.seq).max()
        };

        if let Some(seq) = seq {
            creators.retain(|creator| creator.seq == seq);
        }
    }
}

fn get_edition_data_from_json<T: DeserializeOwned>(data: serde_json::Value) -> Result<T, DbErr> {
    serde_json::from_value(data).map_err(|e| DbErr::Custom(e.to_string()))
}

fn attachment_to_nft_edition(
    attachment: asset_v1_account_attachments::Model,
) -> Result<NftEdition, DbErr> {
    let data: Edition = attachment
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound("Edition data not found".to_string()))
        .map(get_edition_data_from_json)??;

    Ok(NftEdition {
        mint_address: attachment
            .asset_id
            .clone()
            .map(|id| bs58::encode(id).into_string())
            .unwrap_or("".to_string()),
        edition_number: data.edition,
        edition_address: bs58::encode(attachment.id.clone()).into_string(),
    })
}

pub async fn get_nft_editions(
    conn: &impl ConnectionTrait,
    mint_address: Pubkey,
    pagination: &Pagination,
    limit: u64,
) -> Result<NftEditions, DbErr> {
    let master_edition_pubkey = MasterEdition::find_pda(&mint_address).0;

    // to fetch nft editions associated with a mint we need to fetch the master edition first
    let master_edition =
        asset_v1_account_attachments::Entity::find_by_id(master_edition_pubkey.to_bytes().to_vec())
            .one(conn)
            .await?
            .ok_or(DbErr::RecordNotFound(
                "Master Edition not found".to_string(),
            ))?;

    let master_edition_data: MasterEdition = master_edition
        .data
        .clone()
        .ok_or(DbErr::RecordNotFound(
            "Master Edition data not found".to_string(),
        ))
        .map(get_edition_data_from_json)??;

    let mut stmt = asset_v1_account_attachments::Entity::find();

    stmt = stmt.filter(
        asset_v1_account_attachments::Column::AttachmentType
            .eq(V1AccountAttachments::Edition)
            // The data field is a JSON field that contains the edition data.
            .and(asset_v1_account_attachments::Column::Data.is_not_null())
            // The parent field is a string field that contains the master edition pubkey ( mapping edition to master edition )
            .and(Expr::cust(&format!(
                "data->>'parent' = '{}'",
                master_edition_pubkey
            ))),
    );

    let nft_editions = paginate(
        pagination,
        limit,
        stmt,
        Order::Asc,
        asset_v1_account_attachments::Column::Id,
    )
    .all(conn)
    .await?
    .into_iter()
    .map(attachment_to_nft_edition)
    .collect::<Result<Vec<NftEdition>, _>>()?;

    let (page, before, after, cursor) = match pagination {
        Pagination::Keyset { before, after } => {
            let bef = before.clone().and_then(|x| String::from_utf8(x).ok());
            let aft = after.clone().and_then(|x| String::from_utf8(x).ok());
            (None, bef, aft, None)
        }
        Pagination::Page { page } => (Some(*page as u32), None, None, None),
        Pagination::Cursor(_) => {
            if let Some(last_asset) = nft_editions.last() {
                let cursor_str = last_asset.edition_address.clone();
                (None, None, None, Some(cursor_str))
            } else {
                (None, None, None, None)
            }
        }
    };

    Ok(NftEditions {
        total: nft_editions.len() as u32,
        master_edition_address: master_edition_pubkey.to_string(),
        supply: master_edition_data.supply,
        max_supply: master_edition_data.max_supply,
        editions: nft_editions,
        limit: limit as u32,
        page,
        before,
        after,
        cursor,
    })
}

async fn get_inscription_by_mint(
    conn: &impl ConnectionTrait,
    mint: Vec<u8>,
) -> Result<asset_v1_account_attachments::Model, DbErr> {
    asset_v1_account_attachments::Entity::find()
        .filter(
            asset_v1_account_attachments::Column::Data
                .is_not_null()
                .and(Expr::cust(&format!(
                    "data->>'root' = '{}'",
                    bs58::encode(mint).into_string()
                ))),
        )
        .one(conn)
        .await
        .and_then(|o| match o {
            Some(t) => Ok(t),
            _ => Err(DbErr::RecordNotFound("Inscription Not Found".to_string())),
        })
}
