use super::slot::get_latest_slot;
use crate::{
    dao::{extensions::token_accounts::TokenAccountSelectExt, token_accounts, tokens, Pagination},
    rpc::{
        options::Options, RpcAccountData, RpcAccountDataInner, RpcData, RpcParsedAccount,
        RpcTokenAccountBalance, RpcTokenAccountBalanceWithAddress, RpcTokenInfo,
        RpcTokenInfoWithDelegate, RpcTokenSupply, SolanaRpcResponse, UiTokenAmount,
    },
};
use num_traits::ToPrimitive;
use sea_orm::{entity::*, query::*, ConnectionTrait, DbErr, Order};
use std::ops::Div;

pub const SPL_TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const SPL_TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const WRAPPED_SOL: &str = "So11111111111111111111111111111111111111112";

pub async fn get_token_accounts(
    conn: &impl ConnectionTrait,
    owner_address: Option<Vec<u8>>,
    mint_address: Option<Vec<u8>>,
    pagination: &Pagination,
    limit: u64,
    options: &Options,
) -> Result<Vec<token_accounts::Model>, DbErr> {
    let mut condition = Condition::all();

    if options.show_zero_balance {
        condition = condition.add(token_accounts::Column::Amount.gte(0));
    } else {
        condition = condition.add(token_accounts::Column::Amount.gt(0));
    }

    if owner_address.is_none() && mint_address.is_none() {
        return Err(DbErr::Custom(
            "Either 'ownerAddress' or 'mintAddress' must be provided".to_string(),
        ));
    }

    if let Some(owner) = owner_address {
        condition = condition.add(token_accounts::Column::Owner.eq(owner));
    }
    if let Some(mint) = mint_address {
        condition = condition.add(token_accounts::Column::Mint.eq(mint));
    }

    let mut stmt = token_accounts::Entity::find().filter(condition);

    stmt = stmt
        .sort_by(token_accounts::Column::Pubkey, &Order::Asc)
        .page_by(
            pagination,
            limit,
            &Order::Asc,
            token_accounts::Column::Pubkey,
        );

    let token_accounts = stmt.all(conn).await?;

    Ok(token_accounts)
}

pub async fn get_token_largest_accounts(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponse<Vec<RpcTokenAccountBalanceWithAddress>>, DbErr> {
    let largest_token_accounts = token_accounts::Entity::find()
        .find_also_related(tokens::Entity)
        .filter(token_accounts::Column::Mint.eq(mint_address))
        .order_by_desc(token_accounts::Column::Amount)
        .order_by_desc(token_accounts::Column::Pubkey)
        .limit(20) // Select the top 20 largest token accounts
        .all(conn)
        .await?;

    let value = largest_token_accounts
        .into_iter()
        .filter_map(|(ta, m)| {
            m.map(|mint| {
                let ui_amount: f64 = (ta.amount as f64) / (10u64.pow(mint.decimals as u32) as f64);

                RpcTokenAccountBalanceWithAddress {
                    address: bs58::encode(ta.pubkey).into_string(),
                    amount: UiTokenAmount {
                        ui_amount: Some(ui_amount),
                        decimals: mint.decimals as u8,
                        amount: ta.amount.to_string(),
                        ui_amount_string: ui_amount.to_string(),
                    },
                }
            })
        })
        .collect();

    let slot = get_latest_slot(conn).await?;

    Ok(SolanaRpcResponse::new(value, slot))
}

pub async fn get_token_supply(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponse<RpcTokenSupply>, DbErr> {
    let token = tokens::Entity::find()
        .filter(tokens::Column::Mint.eq(mint_address))
        .one(conn)
        .await?
        .ok_or(DbErr::RecordNotFound("Token Not Found".to_string()))?;

    let ui_supply = token
        .supply
        .to_f64()
        .map_or(0f64, |s| s.div(10u64.pow(token.decimals as u32) as f64));

    let value = RpcTokenSupply {
        amount: token.supply.to_string(),
        decimals: token.decimals as u8,
        ui_amount: Some(ui_supply),
        ui_amount_string: ui_supply.to_string(),
    };

    let slot = get_latest_slot(conn).await?;

    Ok(SolanaRpcResponse::new(value, slot))
}

pub async fn get_token_accounts_by_owner(
    conn: &impl ConnectionTrait,
    owner_address: Vec<u8>,
    mint: Option<Vec<u8>>,
    program_id: Option<Vec<u8>>,
) -> Result<SolanaRpcResponse<Vec<RpcData<RpcTokenInfo>>>, DbErr> {
    let query = token_accounts::Entity::find().find_also_related(tokens::Entity);

    let mut conditions = Condition::all().add(token_accounts::Column::Owner.eq(owner_address));

    if let Some(mint) = mint {
        conditions = conditions.add(token_accounts::Column::Mint.eq(mint));
    }

    if let Some(token_program) = program_id {
        conditions = conditions.add(token_accounts::Column::TokenProgram.eq(token_program));
    }

    let ta_with_mint_deimals = query
        .filter(conditions)
        .order_by(token_accounts::Column::Pubkey, Order::Asc)
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|(ta, maybe_mint)| maybe_mint.map(|m| (ta, m.decimals)))
        .collect::<Vec<_>>();

    let value = ta_with_mint_deimals
        .into_iter()
        .map(|(ta, decimals)| {
            let ui_amount: f64 = (ta.amount as f64).div(10u64.pow(decimals as u32) as f64);

            RpcData {
                pubkey: bs58::encode(ta.pubkey.clone()).into_string(),
                account: RpcAccountData {
                    data: RpcAccountDataInner {
                        program: get_token_program_name(&ta.token_program),
                        parsed: RpcParsedAccount {
                            info: RpcTokenInfo {
                                owner: bs58::encode(ta.owner).into_string(),
                                is_native: is_native_token(&ta.mint),
                                state: "initialized".to_string(),
                                token_amount: RpcTokenAccountBalance {
                                    amount: UiTokenAmount {
                                        ui_amount: Some(ui_amount),
                                        decimals: decimals as u8,
                                        amount: ta.amount.to_string(),
                                        ui_amount_string: ui_amount.to_string(),
                                    },
                                },
                                mint: bs58::encode(ta.mint).into_string(),
                            },
                            account_type: "account".to_string(),
                        },
                        ..Default::default()
                    },
                    owner: bs58::encode(ta.token_program).into_string(),
                    ..Default::default()
                },
            }
        })
        .collect::<Vec<_>>();

    let slot = get_latest_slot(conn).await?;

    Ok(SolanaRpcResponse::new(value, slot))
}

pub async fn get_token_accounts_by_delegate(
    conn: &impl ConnectionTrait,
    delegate_address: Vec<u8>,
    mint_address: Option<Vec<u8>>,
    token_program: Option<Vec<u8>>,
) -> Result<SolanaRpcResponse<Vec<RpcData<RpcTokenInfoWithDelegate>>>, DbErr> {
    let query = token_accounts::Entity::find().find_also_related(tokens::Entity);

    let mut conditions =
        Condition::all().add(token_accounts::Column::Delegate.eq(delegate_address));

    if let Some(token_program) = token_program {
        conditions = conditions.add(token_accounts::Column::TokenProgram.eq(token_program));
    }

    if let Some(ref mint) = mint_address {
        conditions = conditions.add(token_accounts::Column::Mint.eq(mint.clone()));
    }

    let ta_with_mint_decimals = query
        .filter(conditions)
        .order_by(token_accounts::Column::Amount, Order::Desc)
        .order_by(token_accounts::Column::Pubkey, Order::Desc)
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|(ta, maybe_mint)| maybe_mint.map(|mint| (ta, mint.decimals)))
        .collect::<Vec<_>>();

    let value = ta_with_mint_decimals
        .into_iter()
        .map(
            |(ta, decimals)| -> Result<RpcData<RpcTokenInfoWithDelegate>, DbErr> {
                let ui_token_amount: f64 =
                    (ta.amount as f64).div(10u64.pow(decimals as u32) as f64);
                let ui_delegate_amount: f64 =
                    (ta.delegated_amount as f64).div(10u64.pow(decimals as u32) as f64);
                Ok(RpcData {
                    pubkey: bs58::encode(ta.pubkey.clone()).into_string(),
                    account: RpcAccountData {
                        data: RpcAccountDataInner {
                            program: get_token_program_name(&ta.token_program),
                            parsed: RpcParsedAccount {
                                info: RpcTokenInfoWithDelegate {
                                    delegate: bs58::encode(ta.delegate.unwrap_or_default())
                                        .into_string(), // this will always be Some bcos we filtered above
                                    delegated_amount: RpcTokenAccountBalance {
                                        amount: UiTokenAmount {
                                            ui_amount: Some(ui_delegate_amount),
                                            decimals: decimals as u8,
                                            amount: ta.amount.to_string(),
                                            ui_amount_string: ui_delegate_amount.to_string(),
                                        },
                                    },
                                    owner: bs58::encode(ta.owner).into_string(),
                                    is_native: is_native_token(&ta.mint),
                                    state: "initialized".to_string(),
                                    token_amount: RpcTokenAccountBalance {
                                        amount: UiTokenAmount {
                                            ui_amount: Some(ui_token_amount),
                                            decimals: decimals as u8,
                                            amount: ta.amount.to_string(),
                                            ui_amount_string: ui_token_amount.to_string(),
                                        },
                                    },
                                    mint: bs58::encode(ta.mint).into_string(),
                                },
                                account_type: "account".to_string(),
                            },
                            ..Default::default()
                        },

                        owner: bs58::encode(ta.token_program).into_string(),
                        ..Default::default()
                    },
                })
            },
        )
        .collect::<Result<Vec<_>, _>>()?;

    let slot = get_latest_slot(conn).await?;

    Ok(SolanaRpcResponse::new(value, slot))
}

pub fn get_token_program_name(token_program: &Vec<u8>) -> String {
    match bs58::encode(token_program).into_string().as_str() {
        SPL_TOKEN => "spl-token".to_string(),
        SPL_TOKEN_2022 => "spl-token-2022".to_string(),
        _ => "unknown".to_string(),
    }
}

pub fn is_token_program_pubkey(token_program: &Vec<u8>) -> bool {
    let pubkey = bs58::encode(token_program).into_string();
    pubkey.eq(SPL_TOKEN) || pubkey.eq(SPL_TOKEN_2022)
}

pub fn is_native_token(mint: &Vec<u8>) -> bool {
    let mint = bs58::encode(mint).into_string();

    mint.eq(WRAPPED_SOL)
}
