use jsonrpsee::RpcModule;
use log::debug;

use crate::{api::*, error::DasApiError};

pub struct RpcApiBuilder;

impl RpcApiBuilder {
    pub fn build(
        contract: Box<dyn ApiContract>,
    ) -> Result<RpcModule<Box<dyn ApiContract>>, DasApiError> {
        let mut module = RpcModule::new(contract);
        module.register_async_method("healthz", |_rpc_params, rpc_context| async move {
            debug!("Checking Health");
            rpc_context.check_health().await.map_err(Into::into)
        })?;

        module.register_async_method("get_asset_proof", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssetProof>()?;
            rpc_context
                .get_asset_proof(payload)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getAssetProof", "get_asset_proof")?;

        module.register_async_method("get_asset_proofs", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssetProofs>()?;
            rpc_context
                .get_asset_proofs(payload)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getAssetProofs", "get_asset_proofs")?;
        module.register_alias("get_asset_proof_batch", "get_asset_proofs")?;
        module.register_alias("getAssetProofBatch", "get_asset_proofs")?;

        module.register_async_method("get_asset", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAsset>()?;
            rpc_context.get_asset(payload).await.map_err(Into::into)
        })?;
        module.register_alias("getAsset", "get_asset")?;

        module.register_async_method("get_assets", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssets>()?;
            rpc_context.get_assets(payload).await.map_err(Into::into)
        })?;
        module.register_alias("getAssets", "get_assets")?;
        module.register_alias("get_asset_batch", "get_assets")?;
        module.register_alias("getAssetBatch", "get_assets")?;

        module.register_async_method(
            "get_assets_by_owner",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetAssetsByOwner>()?;
                rpc_context
                    .get_assets_by_owner(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getAssetsByOwner", "get_assets_by_owner")?;

        module.register_async_method(
            "get_assets_by_creator",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetAssetsByCreator>()?;
                rpc_context
                    .get_assets_by_creator(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getAssetsByCreator", "get_assets_by_creator")?;

        module.register_async_method(
            "getAssetsByAuthority",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetAssetsByAuthority>()?;
                rpc_context
                    .get_assets_by_authority(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_async_method(
            "get_assets_by_group",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetAssetsByGroup>()?;
                rpc_context
                    .get_assets_by_group(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getAssetsByGroup", "get_assets_by_group")?;

        module.register_async_method(
            "getAssetSignatures",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetAssetSignatures>()?;
                rpc_context
                    .get_asset_signatures(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getSignaturesForAsset", "getAssetSignatures")?;

        module.register_async_method("search_assets", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<SearchAssets>()?;
            rpc_context.search_assets(payload).await.map_err(Into::into)
        })?;
        module.register_alias("searchAssets", "search_assets")?;

        module.register_async_method("schema", |_, rpc_context| async move {
            Ok(rpc_context.schema())
        })?;

        module.register_async_method(
            "get_token_accounts",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetTokenAccounts>()?;
                rpc_context
                    .get_token_accounts(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_alias("getTokenAccounts", "get_token_accounts")?;

        Ok(module)
    }
}
