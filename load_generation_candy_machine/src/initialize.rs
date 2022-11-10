use anchor_client::Client;
use anchor_lang::AccountDeserialize;
use mpl_candy_guard::guards::BotTax;
use mpl_candy_guard::guards::SolPayment;
use mpl_candy_guard::guards::StartDate;
use mpl_candy_guard::state::CandyGuardData;
use mpl_candy_guard::state::GuardSet;
use mpl_candy_machine::{CandyMachine, CandyMachineData, ConfigLine, Creator};
use mpl_candy_machine_core::{
    CandyMachineData as CandyMachineDataV3, ConfigLineSettings, Creator as CandyMachineCreatorV3,
};
use solana_client::{client_error::ClientError, nonblocking::rpc_client::RpcClient};
use solana_program::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey};
use solana_sdk::{signature::Keypair, signer::Signer};
use std::sync::Arc;

use crate::{
    // add_config_lines,
    candy_machine_constants::{DEFAULT_PRICE, DEFAULT_SYMBOL, DEFAULT_UUID},
    initialize_candy_machine,
    initialize_candy_machine_v3,
};

pub async fn make_a_candy_machine(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
) -> Result<Pubkey, ClientError> {
    let (candy_machine, authority, minter) =
        init_candy_machine_info(payer.clone(), solana_client.clone()).await?;

    solana_client
        .clone()
        .request_airdrop(&payer.clone().pubkey(), LAMPORTS_PER_SOL * 100)
        .await?;

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
        solana_client.clone(),
    )
    .await?;

    // TODO add config lines will be finished in next pr P-674
    // add_all_config_lines(&candy_machine.pubkey(), &authority, solana_client.clone()).await?;
    //  candy_manager.set_collection(context).await.unwrap();

    Ok(candy_machine.pubkey())
}

pub async fn make_a_candy_machine_v3(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
) -> Result<Pubkey, ClientError> {
    let (candy_machine, authority, minter) =
        init_candy_machine_info(payer.clone(), solana_client.clone()).await?;

    solana_client
        .clone()
        .request_airdrop(&payer.clone().pubkey(), LAMPORTS_PER_SOL * 100)
        .await?;

    let candy_data = CandyMachineDataV3 {
        items_available: 3,
        symbol: DEFAULT_SYMBOL.to_string(),
        seller_fee_basis_points: 500,
        max_supply: 0,
        is_mutable: true,
        creators: vec![CandyMachineCreatorV3 {
            address: authority.pubkey(),
            verified: true,
            percentage_share: 100,
        }],
        config_line_settings: Some(ConfigLineSettings {
            prefix_name: "TEST".to_string(),
            name_length: 10,
            prefix_uri: "https://arweave.net/".to_string(),
            uri_length: 50,
            is_sequential: false,
        }),
        hidden_settings: None,
    };

    initialize_candy_machine_v3(
        &candy_machine,
        payer.clone(),
        &authority.pubkey(),
        candy_data,
        solana_client.clone(),
    )
    .await?;

    // add_all_config_lines(&candy_machine.pubkey(), &authority, solana_client.clone()).await?;
    //  candy_manager.set_collection(context).await.unwrap();

    wrap_in_candy_guard(&payer.clone(), solana_client.clone()).await?;

    Ok(candy_machine.pubkey())
}

pub async fn wrap_in_candy_guard(
    payer: &Keypair,
    solana_client: Arc<RpcClient>,
) -> Result<(), ClientError> {
    let candy_guard_data = CandyGuardData {
        default: GuardSet {
            bot_tax: Some(BotTax {
                lamports: 100000000,
                last_instruction: true,
            }),
            sol_payment: Some(SolPayment {
                lamports: 100000000,
                destination: payer.pubkey().clone(),
            }),
            token_payment: None,
            start_date: Some(StartDate { date: 1663965742 }),
            third_party_signer: None,
            token_gate: None,
            gatekeeper: None,
            end_date: None,
            allow_list: None,
            mint_limit: None,
            nft_payment: None,
            redeemed_amount: None,
            address_gate: None,
            nft_gate: None,
            nft_burn: None,
            token_burn: None,
            freeze_sol_payment: None,
            freeze_token_payment: None,
            program_gate: None,
        },
        groups: None,
    };

    Ok(())
}

pub fn make_config_lines(start_index: u32, total: u8) -> Vec<ConfigLine> {
    let mut config_lines = Vec::with_capacity(total as usize);
    for i in 0..total {
        config_lines.push(ConfigLine {
            name: format!("Item #{}", i as u32 + start_index),
            uri: format!("Item #{} URI", i as u32 + start_index),
        })
    }
    config_lines
}

pub async fn add_all_config_lines(
    candy_machine: &Pubkey,
    authority: &Keypair,
    solana_client: Arc<RpcClient>,
) -> Result<(), ClientError> {
    solana_client
        .clone()
        .request_airdrop(&authority.pubkey(), LAMPORTS_PER_SOL * 100)
        .await?;

    let candy_machine_account = solana_client.get_account(candy_machine).await?;

    println!("candy {:?}", candy_machine_account);

    let candy_machine_data =
        CandyMachine::try_deserialize(&mut candy_machine_account.data.as_ref()).unwrap();

    let total_items = candy_machine_data.data.items_available;
    for i in 0..total_items / 10 {
        let index = (i * 10) as u32;
        let config_lines = make_config_lines(index, 10);
        // TODO add config lines will be finished in next pr P-674
        // add_config_lines(
        //     candy_machine,
        //     authority,
        //     index,
        //     config_lines,
        //     solana_client.clone(),
        // )
        // .await?;
    }
    let remainder = total_items & 10;
    if remainder > 0 {
        let index = (total_items as u32 / 10).saturating_sub(1);
        let config_lines = make_config_lines(index, remainder as u8);
        // TODO add config lines will be finished in next pr P-674
        // add_config_lines(
        //     candy_machine,
        //     authority,
        //     index,
        //     config_lines,
        //     solana_client.clone(),
        // )
        // .await?;
    }

    Ok(())
}

// TODO make one of these just for cmv3 P-654
pub async fn init_candy_machine_info(
    payer: Arc<Keypair>,
    solana_client: Arc<RpcClient>,
) -> Result<(Keypair, Keypair, Keypair), ClientError> {
    let candy_machine = Keypair::new();
    let authority = Keypair::new();
    let minter = Keypair::new();

    solana_client
        .request_airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 10)
        .await?;

    //TODO all below code P-626
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
