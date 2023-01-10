use digital_asset_types::dapi::{
    get_asset, get_assets_by_authority, get_assets_by_creators, get_assets_by_group,
    get_assets_by_owner, get_proof_for_asset, search_assets, SearchAssetsQuery,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use {
    crate::api::ApiContract,
    crate::config::Config,
    crate::validation::validate_pubkey,
    crate::DasApiError,
    async_trait::async_trait,
    digital_asset_types::rpc::{filter::AssetSorting, response::AssetList, Asset, AssetProof},
    sea_orm::{DatabaseConnection, DbErr, SqlxPostgresConnector},
    sqlx::postgres::PgPoolOptions,
};

pub struct DasApi {
    db_connection: DatabaseConnection,
}

impl DasApi {
    pub async fn from_config(config: Config) -> Result<Self, DasApiError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await?;

        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        Ok(DasApi {
            db_connection: conn,
        })
    }

    fn validate_pagination(
        &self,
        limit: &Option<u32>,
        page: &Option<u32>,
        before: &Option<String>,
        after: &Option<String>,
    ) -> Result<(), DasApiError> {
        if page.is_none() && before.is_none() && after.is_none() {
            return Err(DasApiError::PaginationEmptyError);
        }

        if let Some(limit) = limit {
            // make config item
            if *limit > 1000 {
                return Err(DasApiError::PaginationError);
            }
        }

        if let Some(_page) = page {
            // make config item
            if before.is_some() || after.is_some() {
                return Err(DasApiError::PaginationError);
            }
        }

        if let Some(before) = before {
            validate_pubkey(before.clone())?;
        }

        if let Some(after) = after {
            validate_pubkey(after.clone())?;
        }

        Ok(())
    }
}

pub fn not_found(asset_id: &String) -> DbErr {
    DbErr::RecordNotFound(format!("Asset Proof for {} Not Found", asset_id))
}

#[async_trait]
impl ApiContract for DasApi {
    async fn check_health(self: &DasApi) -> Result<(), DasApiError> {
        self.db_connection
            .execute(Statement::from_string(
                DbBackend::Postgres,
                "SELECT 1".to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn get_asset_proof(self: &DasApi, asset_id: String) -> Result<AssetProof, DasApiError> {
        let id = validate_pubkey(asset_id.clone())?;
        let id_bytes = id.to_bytes().to_vec();
        get_proof_for_asset(&self.db_connection, id_bytes)
            .await
            .and_then(|p| {
                println!("Proof: {:?}", p);
                if p.proof.is_empty() {
                    return Err(not_found(&asset_id));
                }
                Ok(p)
            })
            .map_err(Into::into)
    }

    async fn get_asset(self: &DasApi, asset_id: String) -> Result<Asset, DasApiError> {
        let id = validate_pubkey(asset_id.clone())?;
        let id_bytes = id.to_bytes().to_vec();
        get_asset(&self.db_connection, id_bytes)
            .await
            .map_err(Into::into)
    }

    async fn get_assets_by_owner(
        self: &DasApi,
        owner_address: String,
        sort_by: AssetSorting,
        limit: Option<u32>,
        page: Option<u32>,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<AssetList, DasApiError> {
        let owner_address = validate_pubkey(owner_address.clone())?;
        let owner_address_bytes = owner_address.to_bytes().to_vec();
        self.validate_pagination(&limit, &page, &before, &after)?;

        get_assets_by_owner(
            &self.db_connection,
            owner_address_bytes,
            sort_by,
            limit.map(|x| x as u64).unwrap_or(1000),
            page.map(|x| x as u64),
            before.map(|x| x.as_bytes().to_vec()),
            after.map(|x| x.as_bytes().to_vec()),
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_group(
        self: &DasApi,
        group_key: String,
        group_value: String,
        sort_by: AssetSorting,
        limit: Option<u32>,
        page: Option<u32>,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<AssetList, DasApiError> {
        get_assets_by_group(
            &self.db_connection,
            group_key,
            group_value,
            sort_by,
            limit.map(|x| x as u64).unwrap_or(1000),
            page.map(|x| x as u64),
            before.map(|x| x.as_bytes().to_vec()),
            after.map(|x| x.as_bytes().to_vec()),
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_creator(
        self: &DasApi,
        creator_expression: Vec<String>,
        sort_by: AssetSorting,
        limit: Option<u32>,
        page: Option<u32>,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<AssetList, DasApiError> {
        let creator_addresses = creator_expression
            .into_iter()
            .map(|x| validate_pubkey(x).unwrap().to_bytes().to_vec())
            .collect::<Vec<_>>();

        self.validate_pagination(&limit, &page, &before, &after)?;

        get_assets_by_creators(
            &self.db_connection,
            creator_addresses,
            sort_by,
            limit.map(|x| x as u64).unwrap_or(1000),
            page.map(|x| x as u64),
            before.map(|x| x.as_bytes().to_vec()),
            after.map(|x| x.as_bytes().to_vec()),
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_authority(
        self: &DasApi,
        authority_address: String,
        sort_by: AssetSorting,
        limit: Option<u32>,
        page: Option<u32>,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<AssetList, DasApiError> {
        let authority_address = validate_pubkey(authority_address)
            .unwrap()
            .to_bytes()
            .to_vec();

        self.validate_pagination(&limit, &page, &before, &after)?;

        get_assets_by_authority(
            &self.db_connection,
            authority_address,
            sort_by,
            limit.map(|x| x as u64).unwrap_or(1000),
            page.map(|x| x as u64),
            before.map(|x| x.as_bytes().to_vec()),
            after.map(|x| x.as_bytes().to_vec()),
        )
        .await
        .map_err(Into::into)
    }

    async fn search_assets(
        &self,
        search_expression: SearchAssetsQuery,
        sort_by: AssetSorting,
        limit: Option<u32>,
        page: Option<u32>,
        before: Option<String>,
        after: Option<String>,
    ) -> Result<AssetList, DasApiError> {
        // Deserialize search assets query
        self.validate_pagination(&limit, &page, &before, &after)?;
        // Execute query
        search_assets(
            &self.db_connection,
            search_expression,
            sort_by,
            limit.map(|x| x as u64).unwrap_or(1000),
            page.map(|x| x as u64),
            before.map(|x| x.as_bytes().to_vec()),
            after.map(|x| x.as_bytes().to_vec()),
        )
        .await
        .map_err(Into::into)
    }
}
