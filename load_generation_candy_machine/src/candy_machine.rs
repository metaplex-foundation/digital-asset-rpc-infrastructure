pub use anchor_client::{
    solana_sdk::{
        account::Account,
        commitment_config::{CommitmentConfig, CommitmentLevel},
        native_token::LAMPORTS_PER_SOL,
        pubkey::Pubkey,
        signature::{Keypair, Signature, Signer},
        system_instruction, system_program, sysvar,
        transaction::Transaction,
    },
    Client, ClientError, Program,
};
use anchor_lang::*;
use mpl_candy_machine::{CandyMachineData, Creator};
use std::sync::Arc;

use crate::candy_machine_constants::{DEFAULT_PRICE, DEFAULT_SYMBOL, DEFAULT_UUID};
use crate::helpers::{add_all_config_lines, initialize_candy_machine};

pub async fn make_a_candy_machine(
    program: Arc<Program>,
    payer: Arc<Keypair>,
) -> Result<Pubkey> {
    let (candy_machine, authority, minter) =
        init_candy_machine_info(payer.clone(), program.clone()).await?;

    program
        .clone()
        .rpc()
        .request_airdrop(&payer.clone().pubkey(), LAMPORTS_PER_SOL * 10)
        .unwrap();

    let candy_data = CandyMachineData {
        uuid: DEFAULT_UUID.to_string(),
        items_available: 3,
        price: DEFAULT_PRICE,
        symbol: DEFAULT_SYMBOL.to_string(),
        seller_fee_basis_points: 500,
        max_supply: 0,
        creators: vec![Creator {
            address: authority.pubkey(),
            verified: true,
            share: 100,
        }],
        is_mutable: true,
        retain_authority: true,
        go_live_date: Some(0),
        end_settings: None,
        hidden_settings: None,
        whitelist_mint_settings: None,
        gatekeeper: None,
    };

    initialize_candy_machine(
        &candy_machine,
        &payer,
        &authority.pubkey(),
        candy_data,
        program.clone(),
    )
    .await?;
    add_all_config_lines(&candy_machine.pubkey(), &authority, program.clone()).await?;
    //  candy_manager.set_collection(context).await.unwrap();

    Ok(candy_machine.pubkey())
}

// TODO for future, once candy machine deps are updated, make this match token account load generator i.e. with solana nonblocking client
pub async fn init_candy_machine_info(
    payer: Arc<Keypair>,
    solana_client: Arc<Program>,
) -> Result<(Keypair, Keypair, Keypair)> {
    let candy_machine = Keypair::new();
    let authority = Keypair::new();
    let minter = Keypair::new();

    solana_client
        .rpc()
        .request_airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10)
        .unwrap();

    // let sized = if let Some(sized) = &collection {
    //     *sized
    // } else {
    //     false
    // };

    // let collection_info = CollectionInfo::init(
    //     context,
    //     collection.is_some(),
    //     &candy_machine.pubkey(),
    //     clone_keypair(&authority),
    //     sized,
    // )
    // .await;

    // let token_info = TokenInfo::init(
    //     context,
    //     token,
    //     &authority,
    //     (authority.pubkey(), 10),
    //     (minter.pubkey(), 1),
    // )
    // .await;

    // let freeze_info = match freeze {
    //     Some(config) => {
    //         FreezeInfo::init(
    //             context,
    //             config.set,
    //             &candy_machine.pubkey(),
    //             config.freeze_time,
    //             token_info.mint,
    //         )
    //         .await
    //     }
    //     None => FreezeInfo::init(context, false, &candy_machine.pubkey(), 0, token_info.mint).await,
    // };

    // let whitelist_info = match whitelist {
    //     Some(config) => {
    //         WhitelistInfo::init(
    //             context,
    //             true,
    //             &authority,
    //             config,
    //             (authority.pubkey(), 10),
    //             (minter.pubkey(), 1),
    //         )
    //         .await
    //     }
    //     None => {
    //         WhitelistInfo::init(
    //             context,
    //             false,
    //             &authority,
    //             WhitelistConfig::default(),
    //             (authority.pubkey(), 10),
    //             (minter.pubkey(), 1),
    //         )
    //         .await
    //     }
    // };

    // let gateway_info = match gatekeeper {
    //     Some(config) => {
    //         GatekeeperInfo::init(
    //             true,
    //             config.gateway_app,
    //             config.gateway_token_info,
    //             config.gatekeeper_config,
    //             minter.pubkey(),
    //         )
    //         .await
    //     }
    //     None => {
    //         GatekeeperInfo::init(
    //             false,
    //             Pubkey::from_str("gatem74V238djXdzWnJf94Wo1DcnuGkfijbf3AuBhfs").unwrap(),
    //             Pubkey::from_str("ignREusXmGrscGNUesoU9mxfds9AiYTezUKex2PsZV6").unwrap(),
    //             GatekeeperConfig::default(),
    //             minter.pubkey(),
    //         )
    //         .await
    //     }
    // };

    // let wallet = match &token_info.set {
    //     true => token_info.auth_account,
    //     false => authority.pubkey(),
    // };

    // Ok((
    //     candy_machine,
    //     authority,
    //     wallet,
    //     minter,
    //     collection_info,
    //     token_info,
    //     whitelist_info,
    //     gateway_info,
    //     freeze_info,
    // ))

    Ok((candy_machine, authority, minter))
}
