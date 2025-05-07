use super::{asset::paginate, slot::get_latest_slot};
use crate::{
    dao::{token_accounts, tokens, Pagination},
    rpc::{
        options::Options, RpcTokenAccountBalance, RpcTokenSupply, SolanaRpcContext,
        SolanaRpcResponseAndContext, UiTokenAmount,
    },
};
use num_traits::ToPrimitive;
use sea_orm::{entity::*, query::*, ConnectionTrait, DbErr, Order};
use std::ops::Div;

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

    let token_accounts = paginate(
        pagination,
        limit,
        token_accounts::Entity::find().filter(condition),
        Order::Desc,
        token_accounts::Column::Amount,
    )
    .order_by(token_accounts::Column::Pubkey, Order::Asc)
    .all(conn)
    .await?;

    Ok(token_accounts)
}

pub async fn get_token_largest_accounts(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponseAndContext<Vec<RpcTokenAccountBalance>>, DbErr> {
    let mint_acc = tokens::Entity::find()
        .filter(tokens::Column::Mint.eq(mint_address.clone()))
        .one(conn)
        .await?
        .ok_or(DbErr::RecordNotFound("Mint Account Not Found".to_string()))?;

    let largest_token_accounts = token_accounts::Entity::find()
        .filter(token_accounts::Column::Mint.eq(mint_address))
        .order_by_desc(token_accounts::Column::Amount)
        .limit(20) // Select the top 20 largest token accounts
        .all(conn)
        .await?;

    let value = largest_token_accounts
        .into_iter()
        .map(|ta| {
            let ui_amount: f64 = (ta.amount as f64).div(10u64.pow(mint_acc.decimals as u32) as f64);
            RpcTokenAccountBalance {
                address: bs58::encode(ta.pubkey).into_string(),
                amount: UiTokenAmount {
                    ui_amount: Some(ui_amount),
                    decimals: mint_acc.decimals as u8,
                    amount: ta.amount.to_string(),
                    ui_amount_string: ui_amount.to_string(),
                },
            }
        })
        .collect();

    let slot = get_latest_slot(conn).await?;

    Ok(SolanaRpcResponseAndContext {
        value,
        context: SolanaRpcContext { slot },
    })
}

pub async fn get_token_supply(
    conn: &impl ConnectionTrait,
    mint_address: Vec<u8>,
) -> Result<SolanaRpcResponseAndContext<RpcTokenSupply>, DbErr> {
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

    Ok(SolanaRpcResponseAndContext {
        value,
        context: SolanaRpcContext { slot },
    })
}
