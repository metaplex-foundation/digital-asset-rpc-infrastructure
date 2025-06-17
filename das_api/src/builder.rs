use jsonrpsee::RpcModule;
use log::debug;

use crate::{
    api::*,
    error::DasApiError,
    metrics::{DasApiMethod, MetricsRecorderExt},
};

pub struct RpcApiBuilder;

impl RpcApiBuilder {
    pub fn build(
        contract: Box<dyn ApiContract>,
    ) -> Result<RpcModule<Box<dyn ApiContract>>, DasApiError> {
        let mut module = RpcModule::new(contract);
        module.register_async_method("healthz", |_rpc_params, rpc_context| async move {
            debug!("Checking Health");
            rpc_context
                .check_health()
                .record_metrics(DasApiMethod::CheckHealth)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getHealth", "healthz")?;
        module.register_async_method("get_slot", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<Option<GetSlot>>()?;
            rpc_context
                .get_slot(payload)
                .record_metrics(DasApiMethod::GetSlot)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getSlot", "get_slot")?;

        module.register_async_method("get_asset_proof", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssetProof>()?;
            rpc_context
                .get_asset_proof(payload)
                .record_metrics(DasApiMethod::GetAssetProof)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getAssetProof", "get_asset_proof")?;

        module.register_async_method("get_asset_proofs", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssetProofs>()?;
            rpc_context
                .get_asset_proofs(payload)
                .record_metrics(DasApiMethod::GetAssetProofs)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getAssetProofs", "get_asset_proofs")?;
        module.register_alias("get_asset_proof_batch", "get_asset_proofs")?;
        module.register_alias("getAssetProofBatch", "get_asset_proofs")?;

        module.register_async_method("get_asset", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAsset>()?;
            rpc_context
                .get_asset(payload)
                .record_metrics(DasApiMethod::GetAsset)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getAsset", "get_asset")?;

        module.register_async_method("get_assets", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetAssets>()?;
            rpc_context
                .get_assets(payload)
                .record_metrics(DasApiMethod::GetAssets)
                .await
                .map_err(Into::into)
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
                    .record_metrics(DasApiMethod::GetAssetsByOwner)
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
                    .record_metrics(DasApiMethod::GetAssetsByCreator)
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
                    .record_metrics(DasApiMethod::GetAssetsByAuthority)
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
                    .record_metrics(DasApiMethod::GetAssetsByGroup)
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
                    .record_metrics(DasApiMethod::GetAssetSignatures)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getSignaturesForAsset", "getAssetSignatures")?;

        module.register_async_method("search_assets", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<SearchAssets>()?;
            rpc_context
                .search_assets(payload)
                .record_metrics(DasApiMethod::SearchAssets)
                .await
                .map_err(Into::into)
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
                    .record_metrics(DasApiMethod::GetTokenAccounts)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getTokenAccounts", "get_token_accounts")?;

        module.register_async_method("get_nft_editions", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetNftEditions>()?;
            rpc_context
                .get_nft_editions(payload)
                .record_metrics(DasApiMethod::GetNftEditions)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getNftEditions", "get_nft_editions")?;

        module.register_async_method(
            "get_token_largest_accounts",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetTokenLargestAccounts>()?;
                rpc_context
                    .get_token_largest_accounts(payload)
                    .record_metrics(DasApiMethod::GetTokenLargestAccounts)
                    .await
                    .map_err(Into::into)
            },
        )?;
        module.register_alias("getTokenLargestAccounts", "get_token_largest_accounts")?;

        module.register_async_method("get_token_supply", |rpc_params, rpc_context| async move {
            let payload = rpc_params.parse::<GetTokenSupply>()?;
            rpc_context
                .get_token_supply(payload)
                .record_metrics(DasApiMethod::GetTokenSupply)
                .await
                .map_err(Into::into)
        })?;
        module.register_alias("getTokenSupply", "get_token_supply")?;

        module.register_async_method(
            "get_token_accounts_by_owner",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetTokenAccountsByOwner>()?;
                rpc_context
                    .get_token_accounts_by_owner(payload)
                    .record_metrics(DasApiMethod::GetTokenAccountsByOwner)
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_alias("getTokenAccountsByOwner", "get_token_accounts_by_owner")?;

        module.register_async_method(
            "get_token_accounts_by_delegate",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetTokenAccountsByDelegate>()?;
                rpc_context
                    .get_token_accounts_by_delegate(payload)
                    .record_metrics(DasApiMethod::GetTokenAccountsByDelegate)
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_alias(
            "getTokenAccountsByDelegate",
            "get_token_accounts_by_delegate",
        )?;

        Ok(module)
    }
}
