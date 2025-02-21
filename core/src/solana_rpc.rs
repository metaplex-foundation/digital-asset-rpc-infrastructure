use anyhow::Result;
use backon::ExponentialBuilder;
use backon::Retryable;
use clap::Parser;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_response::RpcTokenAccountBalance;
use solana_client::{
    client_error::ClientError,
    nonblocking::rpc_client::RpcClient,
    rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig},
    rpc_filter::RpcFilterType,
    rpc_request::RpcRequest,
    rpc_response::Response as RpcResponse,
    rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use solana_transaction_status::UiTransactionEncoding;
use std::sync::Arc;

#[derive(Clone, Parser, Debug)]
pub struct SolanaRpcArgs {
    #[arg(long, env)]
    pub solana_rpc_url: String,
}

#[derive(Clone)]
pub struct Rpc(Arc<RpcClient>);

impl Rpc {
    pub fn from_config(config: &SolanaRpcArgs) -> Self {
        Rpc(Arc::new(RpcClient::new(config.solana_rpc_url.clone())))
    }

    pub async fn get_transaction(
        &self,
        signature: &Signature,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta, ClientError> {
        (|| async {
            self.0
                .get_transaction_with_config(
                    signature,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Base58),
                        max_supported_transaction_version: Some(0),
                        commitment: Some(CommitmentConfig {
                            commitment: CommitmentLevel::Finalized,
                        }),
                    },
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    pub async fn get_signatures_for_address(
        &self,
        pubkey: &Pubkey,
        before: Option<Signature>,
        until: Option<Signature>,
        limit: Option<usize>,
    ) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>, ClientError> {
        (|| async {
            self.0
                .get_signatures_for_address_with_config(
                    pubkey,
                    GetConfirmedSignaturesForAddress2Config {
                        before,
                        until,
                        limit,
                        commitment: Some(CommitmentConfig {
                            commitment: CommitmentLevel::Finalized,
                        }),
                    },
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    pub async fn get_account(
        &self,
        pubkey: &Pubkey,
    ) -> Result<
        solana_client::rpc_response::Response<std::option::Option<solana_sdk::account::Account>>,
        ClientError,
    > {
        (|| async {
            self.0
                .get_account_with_config(
                    pubkey,
                    RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig {
                            commitment: CommitmentLevel::Finalized,
                        }),
                        ..RpcAccountInfoConfig::default()
                    },
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    pub async fn get_program_accounts(
        &self,
        program: &Pubkey,
        filters: Option<Vec<RpcFilterType>>,
    ) -> Result<Vec<(Pubkey, Account)>, ClientError> {
        (|| async {
            let filters = filters.clone();

            self.0
                .get_program_accounts_with_config(
                    program,
                    RpcProgramAccountsConfig {
                        filters,
                        account_config: RpcAccountInfoConfig {
                            encoding: Some(UiAccountEncoding::Base64),
                            commitment: Some(CommitmentConfig {
                                commitment: CommitmentLevel::Finalized,
                            }),
                            ..RpcAccountInfoConfig::default()
                        },
                        ..RpcProgramAccountsConfig::default()
                    },
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await
    }

    pub async fn get_multiple_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> Result<Vec<Option<Account>>, ClientError> {
        Ok((|| async {
            self.0
                .get_multiple_accounts_with_config(
                    pubkeys,
                    RpcAccountInfoConfig {
                        commitment: Some(CommitmentConfig {
                            commitment: CommitmentLevel::Finalized,
                        }),
                        ..RpcAccountInfoConfig::default()
                    },
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await?
        .value)
    }

    pub async fn get_token_largest_account(&self, mint: Pubkey) -> anyhow::Result<Pubkey> {
        Ok((|| async {
            self.0
                .send::<RpcResponse<Vec<RpcTokenAccountBalance>>>(
                    RpcRequest::Custom {
                        method: "getTokenLargestAccounts",
                    },
                    serde_json::json!([mint.to_string(),]),
                )
                .await
        })
        .retry(&ExponentialBuilder::default())
        .await?
        .value
        .first()
        .ok_or(anyhow::anyhow!(format!(
            "no token accounts for mint {mint}: burned nft?"
        )))?
        .address
        .parse::<Pubkey>()?)
    }
}
