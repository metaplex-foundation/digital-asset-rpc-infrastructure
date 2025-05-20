use crate::error::DasApiError;
use crate::validation::{validate_opt_pubkey, validate_search_with_name};
use digital_asset_types::{
    dao::{
        scopes::{
            grouping::get_grouping,
            nft_edition::get_nft_editions,
            token::{get_token_accounts_by_owner, get_token_largest_accounts, get_token_supply},
        },
        sea_orm_active_enums::{
            OwnerType, RoyaltyTargetType, SpecificationAssetClass, SpecificationVersions,
        },
        Cursor, PageOptions, SearchAssetsQuery,
    },
    dapi::{
        get_asset, get_asset_proofs, get_asset_signatures, get_assets, get_assets_by_authority,
        get_assets_by_creator, get_assets_by_group, get_assets_by_owner, get_proof_for_asset,
        get_token_accounts, search_assets,
    },
    rpc::{
        filter::{AssetSortBy, SearchConditionType},
        response::{GetGroupingResponse, TokenAccountList},
        OwnershipModel, RoyaltyModel, SolanaRpcResponse,
    },
};
use open_rpc_derive::document_rpc;
use open_rpc_schema::document::OpenrpcDocument;
use sea_orm::{sea_query::ConditionType, ConnectionTrait, DbBackend, Statement};
use sqlx::{Pool, Postgres};
use {
    crate::api::*,
    crate::config::Config,
    crate::validation::validate_pubkey,
    async_trait::async_trait,
    digital_asset_types::rpc::{response::AssetList, Asset, AssetProof},
    sea_orm::{DatabaseConnection, DbErr, SqlxPostgresConnector},
    sqlx::postgres::PgPoolOptions,
};

use digital_asset_types::rpc::RpcTokenAccountBalanceWithAddress;

pub struct DasApi {
    pool: Pool<Postgres>,
}

impl DasApi {
    pub async fn from_config(config: Config) -> Result<Self, DasApiError> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_database_connections.unwrap_or(250))
            .connect(&config.database_url)
            .await?;

        Ok(DasApi { pool })
    }

    pub fn get_connection(&self) -> DatabaseConnection {
        SqlxPostgresConnector::from_sqlx_postgres_pool(self.pool.clone())
    }

    fn get_cursor(&self, cursor: &Option<String>) -> Result<Cursor, DasApiError> {
        match cursor {
            Some(cursor_b64) => {
                let cursor_vec = bs58::decode(cursor_b64)
                    .into_vec()
                    .map_err(|_| DasApiError::CursorValidationError(cursor_b64.clone()))?;
                let cursor_struct = Cursor {
                    id: Some(cursor_vec),
                };
                Ok(cursor_struct)
            }
            None => Ok(Cursor::default()),
        }
    }

    fn validate_pagination(
        &self,
        limit: Option<u32>,
        page: Option<u32>,
        before: &Option<String>,
        after: &Option<String>,
        cursor: &Option<String>,
        sorting: Option<AssetSorting>,
    ) -> Result<PageOptions, DasApiError> {
        let mut is_cursor_enabled = true;
        let mut page_opt = PageOptions::default();

        if let Some(limit) = limit {
            // make config item
            if limit > 1000 {
                return Err(DasApiError::PaginationExceededError);
            }
        }

        if let Some(page) = page {
            if page == 0 {
                return Err(DasApiError::PaginationEmptyError);
            }

            // make config item
            if before.is_some() || after.is_some() || cursor.is_some() {
                return Err(DasApiError::PaginationError);
            }

            is_cursor_enabled = false;
        }

        if let Some(before) = before {
            if cursor.is_some() {
                return Err(DasApiError::PaginationError);
            }
            if let Some(sort) = &sorting {
                if sort.sort_by != AssetSortBy::Id {
                    return Err(DasApiError::PaginationSortingValidationError);
                }
            }
            validate_pubkey(before.clone())?;
            is_cursor_enabled = false;
        }

        if let Some(after) = after {
            if cursor.is_some() {
                return Err(DasApiError::PaginationError);
            }
            if let Some(sort) = &sorting {
                if sort.sort_by != AssetSortBy::Id {
                    return Err(DasApiError::PaginationSortingValidationError);
                }
            }
            validate_pubkey(after.clone())?;
            is_cursor_enabled = false;
        }

        page_opt.limit = limit.map(|x| x as u64).unwrap_or(1000);
        if is_cursor_enabled {
            if let Some(sort) = &sorting {
                if sort.sort_by != AssetSortBy::Id {
                    return Err(DasApiError::PaginationSortingValidationError);
                }
                page_opt.cursor = Some(self.get_cursor(cursor)?);
            }
        } else {
            page_opt.page = page.map(|x| x as u64);
            page_opt.before = before
                .clone()
                .map(|x| bs58::decode(x).into_vec().unwrap_or_default());
            page_opt.after = after
                .clone()
                .map(|x| bs58::decode(x).into_vec().unwrap_or_default());
        }
        Ok(page_opt)
    }
}

pub fn not_found(asset_id: &String) -> DbErr {
    DbErr::RecordNotFound(format!("Asset Proof for {} Not Found", asset_id))
}

#[document_rpc]
#[async_trait]
impl ApiContract for DasApi {
    async fn check_health(self: &DasApi) -> Result<(), DasApiError> {
        self.get_connection()
            .execute(Statement::from_string(
                DbBackend::Postgres,
                "SELECT 1".to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn get_asset_proof(
        self: &DasApi,
        payload: GetAssetProof,
    ) -> Result<AssetProof, DasApiError> {
        let id = validate_pubkey(payload.id.clone())?;
        let id_bytes = id.to_bytes().to_vec();
        get_proof_for_asset(&self.get_connection(), id_bytes)
            .await
            .and_then(|p| {
                if p.proof.is_empty() {
                    return Err(not_found(&payload.id));
                }
                Ok(p)
            })
            .map_err(Into::into)
    }

    async fn get_asset_proofs(
        self: &DasApi,
        payload: GetAssetProofs,
    ) -> Result<HashMap<String, Option<AssetProof>>, DasApiError> {
        let GetAssetProofs { ids } = payload;

        let batch_size = ids.len();
        if batch_size > 1000 {
            return Err(DasApiError::BatchSizeExceededError);
        }

        let id_bytes = ids
            .iter()
            .map(|id| validate_pubkey(id.clone()).map(|id| id.to_bytes().to_vec()))
            .collect::<Result<Vec<Vec<u8>>, _>>()?;

        let proofs = get_asset_proofs(&self.get_connection(), id_bytes).await?;

        let result: HashMap<String, Option<AssetProof>> = ids
            .iter()
            .map(|id| (id.clone(), proofs.get(id).cloned()))
            .collect();
        Ok(result)
    }

    async fn get_asset(self: &DasApi, payload: GetAsset) -> Result<Asset, DasApiError> {
        let GetAsset { id, options } = payload;
        let id_bytes = validate_pubkey(id.clone())?.to_bytes().to_vec();
        let options = options.unwrap_or_default();
        get_asset(&self.get_connection(), id_bytes, &options)
            .await
            .map_err(Into::into)
    }

    async fn get_assets(
        self: &DasApi,
        payload: GetAssets,
    ) -> Result<Vec<Option<Asset>>, DasApiError> {
        let GetAssets { ids, options } = payload;

        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.dedup();

        let batch_size = ids.len();
        if batch_size > 1000 {
            return Err(DasApiError::BatchSizeExceededError);
        }

        let id_bytes = ids
            .iter()
            .map(|id| validate_pubkey(id.clone()).map(|id| id.to_bytes().to_vec()))
            .collect::<Result<Vec<Vec<u8>>, _>>()?;

        let options = options.unwrap_or_default();

        let assets = get_assets(
            &self.get_connection(),
            id_bytes,
            batch_size as u64,
            &options,
        )
        .await?;

        let result: Vec<Option<Asset>> = ids.iter().map(|id| assets.get(id).cloned()).collect();
        Ok(result)
    }

    async fn get_assets_by_owner(
        self: &DasApi,
        payload: GetAssetsByOwner,
    ) -> Result<AssetList, DasApiError> {
        let GetAssetsByOwner {
            owner_address,
            sort_by,
            limit,
            page,
            before,
            after,
            options,
            cursor,
        } = payload;
        let before: Option<String> = before.filter(|before| !before.is_empty());
        let after: Option<String> = after.filter(|after| !after.is_empty());
        let owner_address = validate_pubkey(owner_address.clone())?;
        let owner_address_bytes = owner_address.to_bytes().to_vec();
        let sort_by = sort_by.unwrap_or_default();
        let options = options.unwrap_or_default();
        let page_options =
            self.validate_pagination(limit, page, &before, &after, &cursor, Some(sort_by))?;
        get_assets_by_owner(
            &self.get_connection(),
            owner_address_bytes,
            sort_by,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_group(
        self: &DasApi,
        payload: GetAssetsByGroup,
    ) -> Result<AssetList, DasApiError> {
        let GetAssetsByGroup {
            group_key,
            group_value,
            sort_by,
            limit,
            page,
            before,
            after,
            options,
            cursor,
        } = payload;
        validate_pubkey(group_value.clone())?;
        let before: Option<String> = before.filter(|before| !before.is_empty());
        let after: Option<String> = after.filter(|after| !after.is_empty());
        let sort_by = sort_by.unwrap_or_default();
        let options = options.unwrap_or_default();
        let page_options =
            self.validate_pagination(limit, page, &before, &after, &cursor, Some(sort_by))?;
        get_assets_by_group(
            &self.get_connection(),
            group_key,
            group_value,
            sort_by,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_creator(
        self: &DasApi,
        payload: GetAssetsByCreator,
    ) -> Result<AssetList, DasApiError> {
        let GetAssetsByCreator {
            creator_address,
            only_verified,
            sort_by,
            limit,
            page,
            before,
            after,
            options,
            cursor,
        } = payload;
        let creator_address = validate_pubkey(creator_address.clone())?;
        let creator_address_bytes = creator_address.to_bytes().to_vec();

        let sort_by = sort_by.unwrap_or_default();
        let page_options =
            self.validate_pagination(limit, page, &before, &after, &cursor, Some(sort_by))?;
        let only_verified = only_verified.unwrap_or_default();
        let options = options.unwrap_or_default();
        get_assets_by_creator(
            &self.get_connection(),
            creator_address_bytes,
            only_verified,
            sort_by,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_authority(
        self: &DasApi,
        payload: GetAssetsByAuthority,
    ) -> Result<AssetList, DasApiError> {
        let GetAssetsByAuthority {
            authority_address,
            sort_by,
            limit,
            page,
            before,
            after,
            options,
            cursor,
        } = payload;
        let sort_by = sort_by.unwrap_or_default();
        let authority_address = validate_pubkey(authority_address.clone())?;
        let authority_address_bytes = authority_address.to_bytes().to_vec();
        let options = options.unwrap_or_default();

        let page_options =
            self.validate_pagination(limit, page, &before, &after, &cursor, Some(sort_by))?;
        get_assets_by_authority(
            &self.get_connection(),
            authority_address_bytes,
            sort_by,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn search_assets(&self, payload: SearchAssets) -> Result<AssetList, DasApiError> {
        let SearchAssets {
            negate,
            condition_type,
            interface,
            owner_address,
            owner_type,
            creator_address,
            creator_verified,
            authority_address,
            grouping,
            delegate,
            frozen,
            supply,
            supply_mint,
            compressed,
            compressible,
            royalty_target_type,
            royalty_target,
            royalty_amount,
            burnt,
            sort_by,
            limit,
            page,
            before,
            after,
            json_uri,
            options,
            cursor,
            name,
            token_type,
        } = payload;

        // Deserialize search assets query
        let spec: Option<(SpecificationVersions, SpecificationAssetClass)> =
            interface.clone().map(|x| x.into());
        let specification_version = spec.clone().map(|x| x.0);
        let specification_asset_class = spec.map(|x| x.1);
        let condition_type = condition_type.map(|x| match x {
            SearchConditionType::Any => ConditionType::Any,
            SearchConditionType::All => ConditionType::All,
        });
        let owner_address = validate_opt_pubkey(&owner_address)?;
        let name = validate_search_with_name(&name, &owner_address)?;
        let creator_address = validate_opt_pubkey(&creator_address)?;
        let delegate = validate_opt_pubkey(&delegate)?;

        let authority_address = validate_opt_pubkey(&authority_address)?;
        let supply_mint = validate_opt_pubkey(&supply_mint)?;
        let royalty_target = validate_opt_pubkey(&royalty_target)?;

        let owner_type = owner_type.map(|x| match x {
            OwnershipModel::Single => OwnerType::Single,
            OwnershipModel::Token => OwnerType::Token,
        });
        let royalty_target_type = royalty_target_type.map(|x| match x {
            RoyaltyModel::Creators => RoyaltyTargetType::Creators,
            RoyaltyModel::Fanout => RoyaltyTargetType::Fanout,
            RoyaltyModel::Single => RoyaltyTargetType::Single,
        });
        let saq = SearchAssetsQuery {
            negate,
            condition_type,
            interface,
            specification_version,
            specification_asset_class,
            owner_address,
            owner_type,
            creator_address,
            creator_verified,
            authority_address,
            grouping,
            delegate,
            frozen,
            supply,
            supply_mint,
            compressed,
            compressible,
            royalty_target_type,
            royalty_target,
            royalty_amount,
            burnt,
            json_uri,
            name,
            token_type,
        };
        let options = options.unwrap_or_default();
        let sort_by = sort_by.unwrap_or_default();
        let page_options =
            self.validate_pagination(limit, page, &before, &after, &cursor, Some(sort_by))?;
        // Execute query
        search_assets(
            &self.get_connection(),
            saq,
            sort_by,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_asset_signatures(
        self: &DasApi,
        payload: GetAssetSignatures,
    ) -> Result<TransactionSignatureList, DasApiError> {
        let GetAssetSignatures {
            id,
            limit,
            page,
            before,
            after,
            tree,
            leaf_index,
            cursor,
            sort_direction,
        } = payload;

        if !((id.is_some() && tree.is_none() && leaf_index.is_none())
            || (id.is_none() && tree.is_some() && leaf_index.is_some()))
        {
            return Err(DasApiError::ValidationError(
                "Must provide either 'id' or both 'tree' and 'leafIndex'".to_string(),
            ));
        }
        let id = validate_opt_pubkey(&id)?;
        let tree = validate_opt_pubkey(&tree)?;

        let page_options = self.validate_pagination(limit, page, &before, &after, &cursor, None)?;

        get_asset_signatures(
            &self.get_connection(),
            id,
            tree,
            leaf_index,
            &page_options,
            sort_direction,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_grouping(
        self: &DasApi,
        payload: GetGrouping,
    ) -> Result<GetGroupingResponse, DasApiError> {
        let GetGrouping {
            group_key,
            group_value,
        } = payload;
        let gs = get_grouping(
            &self.get_connection(),
            group_key.clone(),
            group_value.clone(),
        )
        .await?;
        Ok(GetGroupingResponse {
            group_key,
            group_name: group_value,
            group_size: gs.size,
        })
    }

    async fn get_token_accounts(
        self: &DasApi,
        payload: GetTokenAccounts,
    ) -> Result<TokenAccountList, DasApiError> {
        let GetTokenAccounts {
            owner_address,
            mint_address,
            limit,
            page,
            before,
            after,
            options,
            cursor,
        } = payload;
        let owner_address = validate_opt_pubkey(&owner_address)?;
        let mint_address = validate_opt_pubkey(&mint_address)?;
        let options = options.unwrap_or_default();
        let page_options = self.validate_pagination(limit, page, &before, &after, &cursor, None)?;
        get_token_accounts(
            &self.get_connection(),
            owner_address,
            mint_address,
            &page_options,
            &options,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_nft_editions(
        self: &DasApi,
        payload: GetNftEditions,
    ) -> Result<NftEditions, DasApiError> {
        let GetNftEditions {
            mint_address,
            page,
            limit,
            before,
            after,
            cursor,
        } = payload;

        let page_options =
            &self.validate_pagination(limit, page, &before, &after, &cursor, None)?;
        let mint_address = validate_pubkey(mint_address.clone())?;
        let pagination = page_options.try_into()?;

        get_nft_editions(
            &self.get_connection(),
            mint_address,
            &pagination,
            page_options.limit,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_token_largest_accounts(
        self: &DasApi,
        payload: GetTokenLargestAccounts,
    ) -> Result<SolanaRpcResponse<Vec<RpcTokenAccountBalanceWithAddress>>, DasApiError> {
        let GetTokenLargestAccounts(mint_address, _d) = payload;
        let mint_address = validate_pubkey(mint_address.clone())?;

        get_token_largest_accounts(&self.get_connection(), mint_address.to_bytes().to_vec())
            .await
            .map_err(Into::into)
    }

    async fn get_token_supply(
        self: &DasApi,
        payload: GetTokenSupply,
    ) -> Result<SolanaRpcResponse<RpcTokenSupply>, DasApiError> {
        let GetTokenSupply(mint_address, _d) = payload;
        let mint_address = validate_pubkey(mint_address.clone())?;

        get_token_supply(&self.get_connection(), mint_address.to_bytes().to_vec())
            .await
            .map_err(Into::into)
    }

    async fn get_token_accounts_by_owner(
        self: &DasApi,
        payload: GetTokenAccountsByOwner,
    ) -> Result<SolanaRpcResponse<Vec<RpcData<RpcTokenInfo>>>, DasApiError> {
        let GetTokenAccountsByOwner(owner, params, _config) = payload;
        let owner_address = validate_pubkey(owner.clone())?;
        let ValidatedTokenAccountParams { mint, program_id } =
            ValidatedTokenAccountParams::try_from(&params)?;

        get_token_accounts_by_owner(
            &self.get_connection(),
            owner_address.to_bytes().to_vec(),
            mint,
            program_id,
        )
        .await
        .map_err(Into::into)
    }
}
