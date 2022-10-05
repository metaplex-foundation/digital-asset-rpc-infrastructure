use crate::{DasApiError, RpcModule};
use async_trait::async_trait;
use digital_asset_types::rpc::filter::{
    AssetSorting, CandyMachineSorting, ListingSorting, OfferSorting,
};
use digital_asset_types::rpc::response::{AssetList, CandyMachineList, ListingsList, OfferList};
use digital_asset_types::rpc::{Asset, AssetProof, CandyMachine};

#[async_trait]
pub trait ApiContract: Send + Sync + 'static {
    async fn check_health(&self) -> Result<(), DasApiError>;
    async fn get_asset_proof(&self, asset_id: String) -> Result<AssetProof, DasApiError>;
    async fn get_asset(&self, asset_id: String) -> Result<Asset, DasApiError>;
    async fn get_assets_by_owner(
        &self,
        owner_address: String,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError>;
    async fn get_listed_assets_by_owner(
        &self,
        owner_address: String,
        sort_by: ListingSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<ListingsList, DasApiError>;
    async fn get_offers_by_owner(
        &self,
        owner_address: String,
        sort_by: OfferSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<OfferList, DasApiError>;
    async fn get_assets_by_group(
        &self,
        group_expression: Vec<String>,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError>;
    async fn get_assets_by_creator(
        &self,
        creator_expression: Vec<String>,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError>;
    async fn search_assets(
        &mut self,
        search_expression: String,
        sort_by: AssetSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<AssetList, DasApiError>;
    async fn get_candy_machine(
        &self,
        candy_machine_id: String,
    ) -> Result<CandyMachine, DasApiError>;
    async fn get_candy_machines_by_creator(
        &self,
        creator_expression: Vec<String>,
        sort_by: CandyMachineSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<CandyMachineList, DasApiError>;
    async fn get_candy_machines_by_size(
        &self,
        candy_machine_size: u64,
        sort_by: CandyMachineSorting,
        limit: u32,
        page: u32,
        before: String,
        after: String,
    ) -> Result<CandyMachineList, DasApiError>;
}

pub struct RpcApiBuilder;

impl<'a> RpcApiBuilder {
    pub fn build(
        contract: Box<dyn ApiContract>,
    ) -> Result<RpcModule<Box<dyn ApiContract>>, DasApiError> {
        let mut module = RpcModule::new(contract);
        module.register_async_method("healthz", |rpc_params, rpc_context| async move {
            println!("Checking Health");
            rpc_context.check_health().await.map_err(Into::into)
        })?;
        module.register_async_method("get_asset_proof", |rpc_params, rpc_context| async move {
            let asset_id = rpc_params.one::<String>()?;
            println!("Asset Id {}", asset_id);
            rpc_context
                .get_asset_proof(asset_id)
                .await
                .map_err(Into::into)
        })?;
        module.register_async_method("get_asset", |rpc_params, rpc_context| async move {
            let asset_id = rpc_params.one::<String>()?;
            println!("Asset Id {}", asset_id);
            rpc_context.get_asset(asset_id).await.map_err(Into::into)
        })?;
        module.register_async_method(
            "get_assets_by_owner",
            |rpc_params, rpc_context| async move {
                let (owner_address, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_assets_by_owner(owner_address, sort_by, limit, page, before, after)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_assets_by_creator",
            |rpc_params, rpc_context| async move {
                let (creator_expression, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_assets_by_creator(creator_expression, sort_by, limit, page, before, after)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_assets_by_group",
            |rpc_params, rpc_context| async move {
                let (group_expression, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_assets_by_group(group_expression, sort_by, limit, page, before, after)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_listed_assets_by_owner",
            |rpc_params, rpc_context| async move {
                let (owner_address, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_listed_assets_by_owner(owner_address, sort_by, limit, page, before, after)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_offers_by_owner",
            |rpc_params, rpc_context| async move {
                let (owner_address, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_offers_by_owner(owner_address, sort_by, limit, page, before, after)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_candy_machine",
            |rpc_params, rpc_context| async move {
                let candy_machine_id = rpc_params.one::<String>()?;
                println!("Candy Machine Id {}", candy_machine_id);
                rpc_context
                    .get_asset(candy_machine_id)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_async_method(
            "get_candy_machines_by_creator",
            |rpc_params, rpc_context| async move {
                let (creator_expression, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_candy_machines_by_creator(
                        creator_expression,
                        sort_by,
                        limit,
                        page,
                        before,
                        after,
                    )
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_async_method(
            "get_candy_machines_by_size",
            |rpc_params, rpc_context| async move {
                let (candy_machine_size, sort_by, limit, page, before, after) =
                    rpc_params.parse().unwrap();
                rpc_context
                    .get_candy_machines_by_size(
                        candy_machine_size,
                        sort_by,
                        limit,
                        page,
                        before,
                        after,
                    )
                    .await
                    .map_err(Into::into)
            },
        )?;

        Ok(module)
    }
}
