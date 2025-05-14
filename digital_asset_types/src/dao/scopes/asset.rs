use crate::{
    dao::{
        asset, asset_authority, asset_creators, asset_data, asset_grouping,
        asset_v1_account_attachments, extensions,
        extensions::asset::AssetSelectStatementExt,
        generated::sea_orm_active_enums::OwnerType,
        sea_orm_active_enums::{SpecificationAssetClass, V1AccountAttachments},
        token_accounts, Cursor, FullAsset, Pagination, SearchAssetsQuery,
    },
    rpc::{filter::TokenTypeClass, options::Options},
};
use sea_orm::{
    sea_query::{
        Alias, Condition, ConditionType, Expr, PostgresQueryBuilder, SimpleExpr, UnionType, Value,
    },
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, FromQueryResult, JoinType, ModelTrait, Order,
    QueryFilter, QuerySelect, Statement,
};
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
pub async fn get_by_creator<D>(
    conn: &D,
    creator: Vec<u8>,
    only_verified: bool,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Condition::all()
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Id)
                        .equals(token_accounts::Entity, token_accounts::Column::Mint),
                )
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Owner)
                        .equals(token_accounts::Entity, token_accounts::Column::Owner),
                ),
        )
        .join(
            JoinType::LeftJoin,
            asset_creators::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(asset_creators::Entity, asset_creators::Column::AssetId),
        )
        .and_where(asset_creators::Column::Creator.eq(creator.clone()))
        .and_where(asset_creators::Column::Verified.eq(true))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if only_verified {
        stmt = stmt
            .and_where(asset_creators::Column::Verified.eq(true))
            .to_owned();
    }

    if !options.show_fungible {
        stmt = stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    stmt = stmt.sort_by(sort_by, &sort_direction).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &sort_direction, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, Some(creator)).await
}

#[allow(clippy::too_many_arguments)]
pub async fn get_by_grouping<D>(
    conn: &D,
    group_key: String,
    group_value: String,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Condition::all()
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Id)
                        .equals(token_accounts::Entity, token_accounts::Column::Mint),
                )
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Owner)
                        .equals(token_accounts::Entity, token_accounts::Column::Owner),
                ),
        )
        .join(
            JoinType::LeftJoin,
            asset_grouping::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(asset_grouping::Entity, asset_grouping::Column::AssetId),
        )
        .and_where(
            asset_grouping::Column::GroupKey
                .eq(group_key)
                .and(asset_grouping::Column::GroupValue.eq(group_value)),
        )
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if !options.show_unverified_collections {
        stmt = stmt
            .and_where(
                asset_grouping::Column::Verified
                    .eq(true)
                    .or(asset_grouping::Column::Verified.is_null()),
            )
            .to_owned();
    }

    if !options.show_fungible {
        stmt = stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    stmt = stmt.sort_by(sort_by, &sort_direction).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &sort_direction, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, None).await
}

pub async fn get_assets_by_owner<D>(
    conn: &D,
    owner: Vec<u8>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut token_stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .join(
            JoinType::InnerJoin,
            token_accounts::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(token_accounts::Entity, token_accounts::Column::Mint),
        )
        .and_where(token_accounts::Column::Owner.eq(owner.to_vec()))
        .and_where(token_accounts::Column::Amount.gt(0))
        .to_owned();

    if !options.show_fungible {
        token_stmt = token_stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    let mut stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::val::<Option<Vec<u8>>>(None),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::val::<Option<Vec<u8>>>(None),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::val::<Option<Vec<u8>>>(None),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::val::<Option<i64>>(None),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::val::<Option<bool>>(None),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::val::<Option<Vec<u8>>>(None),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::val::<Option<i64>>(None),
            Alias::new("token_account_delegated_amount"),
        )
        .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
        .and_where(asset::Column::Owner.eq(owner.to_vec()))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    stmt = stmt.union(UnionType::All, token_stmt).to_owned();

    stmt = stmt.sort_by(sort_by, &sort_direction).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &sort_direction, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, None).await
}

pub async fn search_assets<D>(
    conn: &D,
    query: &SearchAssetsQuery,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut stmt = extensions::asset::Row::select().to_owned();

    if let Some(owner) = &query.owner_address {
        stmt = stmt
            .expr_as(
                Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
                Alias::new("token_account_pubkey"),
            )
            .expr_as(
                Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
                Alias::new("token_owner"),
            )
            .expr_as(
                Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
                Alias::new("token_account_delegate"),
            )
            .expr_as(
                Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
                Alias::new("token_account_amount"),
            )
            .expr_as(
                Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
                Alias::new("token_account_frozen"),
            )
            .expr_as(
                Expr::col((
                    token_accounts::Entity,
                    token_accounts::Column::CloseAuthority,
                )),
                Alias::new("token_account_close_authority"),
            )
            .expr_as(
                Expr::col((
                    token_accounts::Entity,
                    token_accounts::Column::DelegatedAmount,
                )),
                Alias::new("token_account_delegated_amount"),
            )
            .join(
                JoinType::LeftJoin,
                token_accounts::Entity,
                Expr::tbl(asset::Entity, asset::Column::Id)
                    .equals(token_accounts::Entity, token_accounts::Column::Mint),
            )
            .cond_where(
                Condition::any()
                    .add(
                        Condition::all()
                            .add(asset::Column::Owner.eq(owner.to_vec()))
                            .add(asset::Column::Supply.gt(0)),
                    )
                    .add(
                        Condition::all()
                            .add(token_accounts::Column::Owner.eq(owner.to_vec()))
                            .add(token_accounts::Column::Amount.gt(0)),
                    ),
            )
            .to_owned();
    } else {
        stmt = stmt
            .expr_as(
                Expr::val::<Option<Vec<u8>>>(None),
                Alias::new("token_account_pubkey"),
            )
            .expr_as(
                Expr::val::<Option<Vec<u8>>>(None),
                Alias::new("token_owner"),
            )
            .expr_as(
                Expr::val::<Option<Vec<u8>>>(None),
                Alias::new("token_account_delegate"),
            )
            .expr_as(
                Expr::val::<Option<i64>>(None),
                Alias::new("token_account_amount"),
            )
            .expr_as(
                Expr::val::<Option<bool>>(None),
                Alias::new("token_account_frozen"),
            )
            .expr_as(
                Expr::val::<Option<Vec<u8>>>(None),
                Alias::new("token_account_close_authority"),
            )
            .expr_as(
                Expr::val::<Option<i64>>(None),
                Alias::new("token_account_delegated_amount"),
            )
            .and_where(asset::Column::Supply.gt(0))
            .to_owned();
    }

    let mut conditions = match &query.condition_type {
        None | Some(ConditionType::All) => Condition::all(),
        Some(ConditionType::Any) => Condition::any(),
    };

    conditions = conditions
        .add_option(
            query
                .specification_version
                .as_ref()
                .map(|x| asset::Column::SpecificationVersion.eq(x.to_owned())),
        )
        .add_option(query.token_type.as_ref().map(|x| {
            match x {
                TokenTypeClass::Compressed => asset::Column::TreeId.is_not_null(),
                TokenTypeClass::Nft | TokenTypeClass::NonFungible => {
                    asset::Column::TreeId.is_null().and(
                        asset::Column::SpecificationAssetClass
                            .eq(SpecificationAssetClass::Nft)
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::MplCoreAsset))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::ProgrammableNft))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::MplCoreCollection))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::NonTransferableNft))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::IdentityNft))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::Print))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::PrintableNft))
                            .or(asset::Column::SpecificationAssetClass
                                .eq(SpecificationAssetClass::TransferRestrictedNft)),
                    )
                }
                TokenTypeClass::Fungible => asset::Column::SpecificationAssetClass
                    .eq(SpecificationAssetClass::FungibleAsset)
                    .or(asset::Column::SpecificationAssetClass
                        .eq(SpecificationAssetClass::FungibleToken)),
                TokenTypeClass::All => asset::Column::SpecificationAssetClass.is_not_null(),
            }
        }))
        .add_option(
            query
                .specification_asset_class
                .as_ref()
                .map(|x| asset::Column::SpecificationAssetClass.eq(x.to_owned())),
        )
        .add_option(
            query
                .token_type
                .as_ref()
                .map(|token_type| match token_type {
                    TokenTypeClass::Fungible => asset::Column::OwnerType.eq(OwnerType::Token),
                    TokenTypeClass::NonFungible | TokenTypeClass::Nft => {
                        asset::Column::OwnerType.eq(OwnerType::Single)
                    }
                    TokenTypeClass::Compressed => asset::Column::TreeId.is_not_null(),
                    TokenTypeClass::All => asset::Column::OwnerType.is_not_null(),
                }),
        )
        .add_option(
            query
                .delegate
                .to_owned()
                .map(|x| asset::Column::Delegate.eq(x)),
        )
        .add_option(query.frozen.map(|x| asset::Column::Frozen.eq(x)))
        .add_option(
            query
                .supply_mint
                .to_owned()
                .map(|x| asset::Column::SupplyMint.eq(x)),
        )
        .add_option(query.compressed.map(|x| asset::Column::Compressed.eq(x)))
        .add_option(
            query
                .compressible
                .map(|x| asset::Column::Compressible.eq(x)),
        )
        .add_option(
            query
                .royalty_target_type
                .to_owned()
                .map(|x| asset::Column::RoyaltyTargetType.eq(x)),
        )
        .add_option(
            query
                .royalty_target
                .to_owned()
                .map(|x| asset::Column::RoyaltyTarget.eq(x)),
        )
        .add_option(
            query
                .royalty_amount
                .map(|x| asset::Column::RoyaltyAmount.eq(x)),
        )
        .add_option(query.burnt.map(|x| asset::Column::Burnt.eq(x)));

    if let Some(s) = query.supply {
        conditions = conditions.add(asset::Column::Supply.eq(s));
    } else {
        conditions = conditions.add(
            asset::Column::Supply
                .ne(0)
                .or(asset::Column::Burnt.eq(true)),
        )
    };

    if let Some(o) = &query.owner_type {
        conditions = conditions.add(asset::Column::OwnerType.eq(o.to_owned()));
    }

    if query.creator_address.is_some() || query.creator_verified.is_some() {
        stmt = stmt
            .join(
                JoinType::InnerJoin,
                asset_creators::Entity,
                Expr::tbl(asset::Entity, asset::Column::Id)
                    .equals(asset_creators::Entity, asset_creators::Column::AssetId),
            )
            .to_owned();
    }

    if let Some(c) = &query.creator_address {
        conditions = conditions.add(asset_creators::Column::Creator.eq(c.to_owned()));
    }

    if let Some(cv) = query.creator_verified {
        conditions = conditions.add(asset_creators::Column::Verified.eq(cv));
    }

    if let Some(a) = query.authority_address.as_ref() {
        stmt = stmt
            .join(
                JoinType::InnerJoin,
                asset_authority::Entity,
                Expr::tbl(asset::Entity, asset::Column::Id)
                    .equals(asset_authority::Entity, asset_authority::Column::AssetId),
            )
            .to_owned();

        conditions = conditions.add(asset_authority::Column::Authority.eq(a.to_owned()));
    }

    if let Some((group_key, group_value)) = &query.grouping {
        stmt = stmt
            .join(
                JoinType::InnerJoin,
                asset_grouping::Entity,
                Expr::tbl(asset::Entity, asset::Column::Id)
                    .equals(asset_grouping::Entity, asset_grouping::Column::AssetId),
            )
            .to_owned();

        let cond = Condition::all()
            .add(asset_grouping::Column::GroupKey.eq(group_key.to_owned()))
            .add(asset_grouping::Column::GroupValue.eq(group_value.to_owned()));

        conditions = conditions.add(cond);
    }

    if let Some(ju) = query.json_uri.as_ref() {
        let cond = Condition::all().add(asset_data::Column::MetadataUrl.eq(ju.to_owned()));
        conditions = conditions.add(cond);
    }

    if let Some(n) = query.name.as_ref() {
        let name_as_str = std::str::from_utf8(&n).map_err(|_| {
            DbErr::Custom("Could not convert raw name bytes into string for comparison".to_owned())
        })?;

        let name_expr = SimpleExpr::Custom(format!("chain_data->>'name' LIKE '%{}%'", name_as_str));

        conditions = conditions.add(name_expr);
    }

    stmt = match query.negate {
        None | Some(false) => stmt.cond_where(conditions).to_owned(),
        Some(true) => stmt.cond_where(conditions.not()).to_owned(),
    };

    stmt = stmt.sort_by(sort_by, &sort_direction).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &sort_direction, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, None).await
}

pub async fn get_assets<D>(
    conn: &D,
    asset_ids: Vec<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Condition::all()
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Id)
                        .equals(token_accounts::Entity, token_accounts::Column::Mint),
                )
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Owner)
                        .equals(token_accounts::Entity, token_accounts::Column::Owner),
                ),
        )
        .and_where(asset::Column::Id.is_in(asset_ids))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if !options.show_fungible {
        stmt = stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    stmt = stmt.order_by(asset::Column::Id, Order::Desc).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &Order::Desc, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, None).await
}

pub async fn get_by_authority<D>(
    conn: &D,
    authority: Vec<u8>,
    sort_by: Option<asset::Column>,
    sort_direction: Order,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Condition::all()
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Id)
                        .equals(token_accounts::Entity, token_accounts::Column::Mint),
                )
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Owner)
                        .equals(token_accounts::Entity, token_accounts::Column::Owner),
                ),
        )
        .join(
            JoinType::LeftJoin,
            asset_authority::Entity,
            Expr::tbl(asset::Entity, asset::Column::Id)
                .equals(asset_authority::Entity, asset_authority::Column::AssetId),
        )
        .and_where(asset_authority::Column::Authority.eq(authority.clone()))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    if !options.show_fungible {
        stmt = stmt
            .and_where(asset::Column::OwnerType.eq(OwnerType::Single))
            .to_owned();
    }

    stmt = stmt.sort_by(sort_by, &sort_direction).to_owned();

    stmt = stmt
        .page_by(pagination, limit, &sort_direction, asset::Column::Id)
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let assets = extensions::asset::Row::find_by_statement(statment)
        .all(conn)
        .await?;

    get_related_for_assets(conn, assets, options, None).await
}

pub async fn get_related_for_assets<D>(
    conn: &D,
    assets: Vec<extensions::asset::Row>,
    options: &Options,
    required_creator: Option<Vec<u8>>,
) -> Result<Vec<FullAsset>, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let mut full_assets = HashMap::new();
    for asset in assets {
        full_assets.insert(
            asset.id.clone(),
            FullAsset {
                asset,
                ..Default::default()
            },
        );
    }

    let ids = full_assets.keys().cloned().collect::<Vec<_>>();

    // Get all creators for all assets in `assets_map` using batch processing
    let creators = asset_creators::Entity::find_batch()
        .batch_in(asset_creators::Column::AssetId, ids)
        .order_by_asc(asset_creators::Column::AssetId)
        .order_by_asc(asset_creators::Column::Position)
        .all(conn)
        .await?;

    // Add the creators to the assets in `asset_map`.
    for c in creators.into_iter() {
        if let Some(asset) = full_assets.get_mut(&c.asset_id) {
            asset.creators.push(c);
        }
    }

    // Filter out stale creators from each asset.
    for (_id, asset) in full_assets.iter_mut() {
        filter_out_stale_creators(&mut asset.creators);
    }

    // If we passed in a required creator, we make sure that creator is still in the creator array
    // of each asset after stale creators were filtered out above.  Only retain those assets that
    // have the required creator.  This corrects `getAssetByCreators` from returning assets for
    // which the required creator is no longer in the creator array.
    if let Some(required) = required_creator {
        full_assets.retain(|_id, asset| asset.creators.iter().any(|c| c.creator == required));
    }

    let ids = full_assets.keys().cloned().collect::<Vec<_>>();

    let authorities = asset_authority::Entity::find_batch()
        .batch_in(asset_authority::Column::AssetId, ids.clone())
        .all(conn)
        .await?;
    for a in authorities.into_iter() {
        if let Some(asset) = full_assets.get_mut(&a.asset_id) {
            asset.authorities.push(a);
        }
    }

    if options.show_inscription {
        let attachments = asset_v1_account_attachments::Entity::find()
            .filter(asset_v1_account_attachments::Column::AssetId.is_in(ids.clone()))
            .filter(
                asset_v1_account_attachments::Column::AttachmentType
                    .eq(V1AccountAttachments::TokenInscription),
            )
            .all(conn)
            .await?;

        for a in attachments.into_iter() {
            if let Some(asset_id) = a.asset_id.as_ref() {
                if let Some(asset) = full_assets.get_mut(asset_id) {
                    asset.inscription = Some(a);
                }
            }
        }
    }

    let cond = if options.show_unverified_collections {
        None
    } else {
        Some(
            Condition::any()
                .add(asset_grouping::Column::Verified.eq(true))
                // Older versions of the indexer did not have the verified flag. A group would be present if and only if it was verified.
                // Therefore if verified is null, we can assume that the group is verified.
                .add(asset_grouping::Column::Verified.is_null()),
        )
    };

    let grouping_base_query = asset_grouping::Entity::find_batch()
        .batch_in(asset_grouping::Column::AssetId, ids.clone())
        .filter(
            Condition::all()
                .add(asset_grouping::Column::GroupValue.is_not_null())
                .add_option(cond),
        );

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

        let asset_data = asset_data::Entity::find_batch()
            .batch_in(asset_data::Column::Id, group_values)
            .all(conn)
            .await?;

        let asset_data_map: HashMap<_, _, RandomState> = HashMap::from_iter(
            asset_data
                .into_iter()
                .map(|ad| (ad.id.clone(), ad))
                .collect::<Vec<_>>(),
        );

        for g in groups.into_iter() {
            if let Some(asset) = full_assets.get_mut(&g.asset_id) {
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
            if let Some(asset) = full_assets.get_mut(&g.asset_id) {
                asset.groups.push((g, None));
            }
        }
    };

    Ok(full_assets.into_iter().map(|(_, v)| v).collect())
}

pub async fn get_by_id<D>(
    conn: &D,
    asset_id: Vec<u8>,
    options: &Options,
) -> Result<FullAsset, DbErr>
where
    D: ConnectionTrait + Send + Sync,
{
    let stmt = extensions::asset::Row::select()
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Pubkey)),
            Alias::new("token_account_pubkey"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Owner)),
            Alias::new("token_owner"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Delegate)),
            Alias::new("token_account_delegate"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Amount)),
            Alias::new("token_account_amount"),
        )
        .expr_as(
            Expr::col((token_accounts::Entity, token_accounts::Column::Frozen)),
            Alias::new("token_account_frozen"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::CloseAuthority,
            )),
            Alias::new("token_account_close_authority"),
        )
        .expr_as(
            Expr::col((
                token_accounts::Entity,
                token_accounts::Column::DelegatedAmount,
            )),
            Alias::new("token_account_delegated_amount"),
        )
        .expr_as(
            Expr::val::<Option<V1AccountAttachments>>(None),
            Alias::new("asset_attachment_type"),
        )
        .expr_as(
            Expr::val::<Option<bool>>(None),
            Alias::new("asset_attachment_initalized"),
        )
        .join(
            JoinType::LeftJoin,
            token_accounts::Entity,
            Condition::all()
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Id)
                        .equals(token_accounts::Entity, token_accounts::Column::Mint),
                )
                .add(
                    Expr::tbl(asset::Entity, asset::Column::Owner)
                        .equals(token_accounts::Entity, token_accounts::Column::Owner),
                ),
        )
        .and_where(asset::Column::Id.eq(asset_id.clone()))
        .and_where(asset::Column::Supply.gt(0))
        .to_owned();

    let (sql, values) = stmt.build(PostgresQueryBuilder);

    let statment = Statement::from_sql_and_values(sea_orm::DatabaseBackend::Postgres, &sql, values);

    let asset = extensions::asset::Row::find_by_statement(statment)
        .one(conn)
        .await?
        .ok_or(DbErr::RecordNotFound("Asset not found".to_string()))?;

    Ok(get_related_for_assets(conn, vec![asset], options, None)
        .await?
        .pop()
        .ok_or(DbErr::RecordNotFound("Asset not found".to_string()))?)
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

pub struct SelectBatch<E, C>
where
    E: EntityTrait,
    C: ColumnTrait + Into<E::Column>,
{
    column: Option<C>,
    batch_size: usize,
    values: Option<Vec<Value>>,
    orderings: Vec<(E::Column, Order)>,
    filters: Vec<Condition>,
}

#[derive(Debug)]
pub enum SelectBatchError {
    NoValuesProvided,
    ColumnNotSet,
    UnsupportedColumnType(String),
    DbError(DbErr),
}

impl From<DbErr> for SelectBatchError {
    fn from(err: DbErr) -> Self {
        SelectBatchError::DbError(err)
    }
}

impl<E, C> SelectBatch<E, C>
where
    E: EntityTrait,
    C: ColumnTrait + Into<E::Column>,
{
    const DEFAULT_BATCH_SIZE: usize = 100;

    pub fn new() -> Self {
        Self {
            column: None,
            batch_size: Self::DEFAULT_BATCH_SIZE,
            values: None,
            orderings: Vec::new(),
            filters: Vec::new(),
        }
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn batch_in<V>(mut self, column: C, values: Vec<V>) -> Self
    where
        V: Into<Value> + Clone,
    {
        let values: Vec<Value> = values.into_iter().map(|v| v.into()).collect();
        self.values = Some(values.clone());
        self.column = Some(column);
        self
    }

    pub fn order_by_asc<O>(mut self, column: O) -> Self
    where
        O: Into<E::Column>,
    {
        self.orderings.push((column.into(), Order::Asc));
        self
    }

    pub fn order_by_desc<O>(mut self, column: O) -> Self
    where
        O: Into<E::Column>,
    {
        self.orderings.push((column.into(), Order::Desc));
        self
    }

    pub fn filter(mut self, filter: Condition) -> Self {
        self.filters.push(filter);
        self
    }

    pub async fn all<'a, D>(self, db: &'a D) -> Result<Vec<E::Model>, SelectBatchError>
    where
        D: ConnectionTrait + Send + Sync + 'a,
    {
        let values = self.values.ok_or(SelectBatchError::NoValuesProvided)?;
        let column = self.column.ok_or(SelectBatchError::ColumnNotSet)?;

        let futures = values.chunks(self.batch_size).map(move |chunk| {
            let mut query = E::find().filter(column.is_in(chunk.to_vec()));
            for filter in &self.filters {
                query = query.filter(filter.clone());
            }
            async move { query.all(db).await.map_err(SelectBatchError::from) }
        });

        let results = futures::future::join_all(futures).await;
        let mut all_models = results
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        if !self.orderings.is_empty() {
            let cmp = |a: &Value, b: &Value| -> Result<std::cmp::Ordering, SelectBatchError> {
                match (a, b) {
                    (Value::Int(a), Value::Int(b)) => Ok(a.cmp(b)),
                    (Value::BigInt(a), Value::BigInt(b)) => Ok(a.cmp(b)),
                    (Value::Double(a), Value::Double(b)) => {
                        Ok(a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    }
                    (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
                    (Value::Bytes(a), Value::Bytes(b)) => Ok(a.cmp(b)),
                    (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
                    (Value::Json(a), Value::Json(b)) => {
                        Ok(format!("{:?}", a).cmp(&format!("{:?}", b)))
                    }
                    _ => Err(SelectBatchError::UnsupportedColumnType(format!(
                        "Cannot sort by column type: {:?}",
                        a
                    ))),
                }
            };
            all_models.sort_by(|a, b| {
                for (col, order) in &self.orderings {
                    let a_val = a.get(col.clone());
                    let b_val = b.get(col.clone());
                    let ordering = match order {
                        Order::Asc => cmp(&a_val, &b_val).unwrap_or(std::cmp::Ordering::Equal),
                        Order::Desc => cmp(&b_val, &a_val).unwrap_or(std::cmp::Ordering::Equal),
                        Order::Field(_) => cmp(&a_val, &b_val).unwrap_or(std::cmp::Ordering::Equal),
                    };
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        Ok(all_models)
    }
}

pub trait EntityBatchExt: EntityTrait {
    fn find_batch<C>() -> SelectBatch<Self, C>
    where
        C: ColumnTrait + Into<Self::Column>;
}

impl<E> EntityBatchExt for E
where
    E: EntityTrait,
{
    fn find_batch<C>() -> SelectBatch<Self, C>
    where
        C: ColumnTrait + Into<Self::Column>,
    {
        SelectBatch::new()
    }
}

impl From<SelectBatchError> for DbErr {
    fn from(err: SelectBatchError) -> Self {
        match err {
            SelectBatchError::DbError(e) => e,
            SelectBatchError::NoValuesProvided => DbErr::Custom("No values provided".to_string()),
            SelectBatchError::ColumnNotSet => DbErr::Custom("Column not set".to_string()),
            SelectBatchError::UnsupportedColumnType(msg) => DbErr::Custom(msg),
        }
    }
}
