use sea_orm::{ConnectionTrait, DbBackend, Statement};
use tokio_postgres::types::ToSql;
use {
    crate::api::ApiContract,
    crate::config::Config,
    crate::validation::validate_pubkey,
    crate::DasApiError,
    async_trait::async_trait,
    digital_asset_types::{
        dapi::{
            asset::*, assets_by_creator::*, assets_by_group::*, assets_by_owner::*,
            candy_machine::*, candy_machines_by_creator::*, change_logs::*,
            listed_assets_by_owner::*, offers_by_owner::*,
        },
        rpc::{
            filter::{AssetSorting, ListingSorting, OfferSorting},
            response::{AssetList, ListingsList, OfferList},
            Asset, AssetProof, CandyMachine,
        },
    },
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
            .connect(&*config.database_url)
            .await?;

        let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
        Ok(DasApi {
            db_connection: conn,
        })
    }
}

pub fn not_found(asset_id: &String) -> DbErr {
    DbErr::RecordNotFound(format!("Asset Proof for {} Not Found", asset_id))
}

#[async_trait]
impl ApiContract for DasApi {
    async fn check_health(self: &DasApi) -> Result<(), DasApiError> {
        &self
            .db_connection
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
                if p.proof.len() == 0 {
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
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError> {
        let owner_address = validate_pubkey(owner_address.clone())?;
        let owner_address_bytes = owner_address.to_bytes().to_vec();
        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };

        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_assets_by_owner(
            &self.db_connection,
            owner_address_bytes,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_listed_assets_by_owner(
        self: &DasApi,
        owner_address: String,
        sort_by: ListingSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<ListingsList, DasApiError> {
        let owner_address = validate_pubkey(owner_address.clone())?;
        let owner_address_bytes = owner_address.to_bytes().to_vec();
        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };

        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_listed_assets_by_owner(
            &self.db_connection,
            owner_address_bytes,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_offers_by_owner(
        self: &DasApi,
        owner_address: String,
        sort_by: OfferSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<OfferList, DasApiError> {
        let owner_address = validate_pubkey(owner_address.clone())?;
        let owner_address_bytes = owner_address.to_bytes().to_vec();
        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };

        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_offers_by_owner(
            &self.db_connection,
            owner_address_bytes,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_group(
        self: &DasApi,
        group_expression: Vec<String>,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError> {
        let group_values = group_expression
            .into_iter()
            .map(|x| validate_pubkey(x).unwrap().to_string())
            .collect::<Vec<String>>();

        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };

        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_assets_by_group(
            &self.db_connection,
            group_values,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_assets_by_creator(
        self: &DasApi,
        creator_expression: Vec<String>,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError> {
        let creator_addresses = creator_expression
            .into_iter()
            .map(|x| validate_pubkey(x).unwrap().to_bytes().to_vec())
            .collect::<Vec<_>>();

        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };
        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_assets_by_creator(
            &self.db_connection,
            creator_addresses,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn search_assets(
        &mut self,
        _search_expression: String,
        _sort_by: AssetSorting,
        _limit: u32,
        _page: u32,
        _before: String,
        _after: String,
    ) -> Result<AssetList, DasApiError> {
        todo!()
    }

    async fn get_candy_machine(
        self: &DasApi,
        candy_machine_id: String,
    ) -> Result<CandyMachine, DasApiError> {
        let id = validate_pubkey(candy_machine_id.clone())?;
        let id_bytes = id.to_bytes().to_vec();
        get_candy_machine(&self.db_connection, id_bytes)
            .await
            .map_err(Into::into)
    }

    async fn get_candy_machines_by_creator(
        self: &DasApi,
        creator_expression: Vec<String>,
        sort_by: CandyMachineSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<CandyMachineList, DasApiError> {
        let creator_addresses = creator_expression
            .into_iter()
            .map(|x| validate_pubkey(x).unwrap().to_bytes().to_vec())
            .collect::<Vec<_>>();

        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };
        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_candy_machines_by_creator(
            &self.db_connection,
            creator_addresses,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }

    async fn get_candy_machines_by_size(
        self: &DasApi,
        candy_machine_size: u64,
        sort_by: CandyMachineSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<CandyMachineList, DasApiError> {
        if page > 0 && (!before.is_empty() || !after.is_empty()) {
            return Err(DasApiError::PaginationError);
        };

        if !before.is_empty() || !after.is_empty() {
            return Err(DasApiError::PaginationError);
        };
        let before = if !before.is_empty() {
            validate_pubkey(before.clone())?.to_bytes().to_vec()
        } else {
            before.as_bytes().to_vec()
        };

        let after = if !after.is_empty() {
            validate_pubkey(after.clone())?.to_bytes().to_vec()
        } else {
            after.as_bytes().to_vec()
        };

        get_candy_machines_by_size(
            &self.db_connection,
            candy_machine_size,
            sort_by,
            limit,
            page,
            before,
            after,
        )
        .await
        .map_err(Into::into)
    }
}
