use anchor_client::{
    solana_sdk::{
        pubkey::Pubkey,
        signature::{Keypair, Signer},
        system_program, sysvar,
        transaction::Transaction,
    },
    Program,
};
use anchor_lang::*;
use mpl_candy_machine::{
    constants::{CONFIG_ARRAY_START, CONFIG_LINE_SIZE},
    CandyMachine, CandyMachineData, ConfigLine,
};
use mpl_token_metadata::{
    instruction,
    state::{Collection, CollectionDetails, Creator, Metadata, Uses, PREFIX},
};
use solana_client::rpc_client::RpcClient;
use solana_program::{instruction::Instruction, program_pack::Pack, system_instruction};
use std::sync::Arc;

pub fn get_ata(owner: Keypair, mint: Keypair) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&owner.pubkey(), &mint.pubkey())
}

pub async fn initialize_candy_machine(
    candy_account: &Keypair,
    payer: &Arc<Keypair>,
    wallet: &Pubkey,
    candy_data: CandyMachineData,
    // token_info: TokenInfo,
    solana_client: Arc<Program>,
) -> Result<()> {
    let items_available = candy_data.items_available;
    let candy_account_size = if candy_data.hidden_settings.is_some() {
        CONFIG_ARRAY_START
    } else {
        CONFIG_ARRAY_START
            + 4
            + items_available as usize * CONFIG_LINE_SIZE
            + 8
            + 2 * (items_available as usize / 8 + 1)
    };

    let lamports = solana_client
        .rpc()
        .get_minimum_balance_for_rent_exemption(candy_account_size)
        .unwrap();

    let create_ix = system_instruction::create_account(
        &payer.pubkey(),
        &candy_account.pubkey(),
        lamports,
        candy_account_size as u64,
        &mpl_candy_machine::id(),
    );

    let mut accounts = mpl_candy_machine::accounts::InitializeCandyMachine {
        candy_machine: candy_account.pubkey(),
        wallet: *wallet,
        authority: payer.pubkey(),
        payer: payer.pubkey(),
        system_program: system_program::id(),
        rent: sysvar::rent::id(),
    }
    .to_account_metas(None);

    // if token_info.set {
    //     accounts.push(AccountMeta::new_readonly(token_info.mint, false));
    // }

    let data = mpl_candy_machine::instruction::InitializeCandyMachine { data: candy_data }.data();

    let init_ix = Instruction {
        program_id: mpl_candy_machine::id(),
        data,
        accounts,
    };

    let latest_blockhash = solana_client.rpc().get_latest_blockhash().unwrap();
    let tx = Transaction::new_signed_with_payer(
        &[create_ix, init_ix],
        Some(&payer.pubkey()),
        &[candy_account, payer],
        latest_blockhash,
    );

    solana_client
        .rpc()
        .send_and_confirm_transaction(&tx)
        .unwrap();

    Ok(())
}

pub async fn add_all_config_lines(
    candy_machine: &Pubkey,
    authority: &Keypair,
    solana_client: Arc<Program>,
) -> Result<()> {
    let candy_machine_account = solana_client.rpc().get_account(candy_machine).unwrap();

    let candy_machine_data: CandyMachine =
        CandyMachine::try_deserialize(&mut candy_machine_account.data.as_ref()).unwrap();
    let total_items = candy_machine_data.data.items_available;
    for i in 0..total_items / 10 {
        let index = (i * 10) as u32;
        let config_lines = make_config_lines(index, 10);
        add_config_lines(
            candy_machine,
            authority,
            index,
            config_lines,
            solana_client.clone(),
        )
        .await?;
    }
    let remainder = total_items & 10;
    if remainder > 0 {
        let index = (total_items as u32 / 10).saturating_sub(1);
        let config_lines = make_config_lines(index, remainder as u8);
        add_config_lines(
            candy_machine,
            authority,
            index,
            config_lines,
            solana_client.clone(),
        )
        .await?;
    }

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

pub async fn add_config_lines(
    candy_machine: &Pubkey,
    authority: &Keypair,
    index: u32,
    config_lines: Vec<ConfigLine>,
    solana_client: Arc<Program>,
) -> Result<()> {
    let accounts = mpl_candy_machine::accounts::AddConfigLines {
        candy_machine: *candy_machine,
        authority: authority.pubkey(),
    }
    .to_account_metas(None);

    let data = mpl_candy_machine::instruction::AddConfigLines {
        index,
        config_lines,
    }
    .data();

    let add_config_line_ix = Instruction {
        program_id: mpl_candy_machine::id(),
        data,
        accounts,
    };

    let tx = Transaction::new_signed_with_payer(
        &[add_config_line_ix],
        Some(&authority.pubkey()),
        &[authority],
        solana_client.rpc().get_latest_blockhash().unwrap(),
    );

    solana_client
        .rpc()
        .send_and_confirm_transaction(&tx)
        .unwrap();

    Ok(())
}

// pub async fn create_mint(
//     authority: &Pubkey,
//     freeze_authority: Option<&Pubkey>,
//     decimals: u8,
//     mint: Option<Keypair>,
// ) -> transport::Result<Keypair> {
//     let mint = mint.unwrap_or_else(Keypair::new);
//     let rent = context.banks_client.get_rent().await.unwrap();

//     let tx = Transaction::new_signed_with_payer(
//         &[
//             system_instruction::create_account(
//                 &payer.pubkey(),
//                 &mint.pubkey(),
//                 rent.minimum_balance(Mint::LEN),
//                 Mint::LEN as u64,
//                 &spl_token::id(),
//             ),
//             spl_token::instruction::initialize_mint(
//                 &spl_token::id(),
//                 &mint.pubkey(),
//                 authority,
//                 freeze_authority,
//                 decimals,
//             )
//             .unwrap(),
//         ],
//         Some(&payer.pubkey()),
//         &[&payer, &mint],
//         context.last_blockhash,
//     );

//     context.banks_client.process_transaction(tx).await.unwrap();
//     Ok(mint)
// }

// pub async fn mint_to_wallets(
//     mint_pubkey: &Pubkey,
//     authority: &Keypair,
//     allocations: Vec<(Pubkey, u64)>,
// ) -> transport::Result<Vec<Pubkey>> {
//     let mut atas = Vec::with_capacity(allocations.len());

//     #[allow(clippy::needless_range_loop)]
//     for i in 0..allocations.len() {
//         let ata = create_associated_token_account(context, &allocations[i].0, mint_pubkey).await?;
//         mint_tokens(authority, mint_pubkey, &ata, allocations[i].1, None).await?;
//         atas.push(ata);
//     }
//     Ok(atas)
// }

// pub async fn mint_tokens(
//     authority: &Keypair,
//     mint: &Pubkey,
//     account: &Pubkey,
//     amount: u64,
//     additional_signer: Option<&Keypair>,
//     payer: &Keypair,
// ) -> transport::Result<()> {
//     let mut signing_keypairs = vec![authority, &payer];
//     if let Some(signer) = additional_signer {
//         signing_keypairs.push(signer);
//     }

//     let ix = spl_token::instruction::mint_to(
//         &spl_token::id(),
//         mint,
//         account,
//         &authority.pubkey(),
//         &[],
//         amount,
//     )
//     .unwrap();

//     let tx = Transaction::new_signed_with_payer(
//         &[ix],
//         Some(&payer.pubkey()),
//         &signing_keypairs,
//         context.last_blockhash,
//     );
//     context.banks_client.process_transaction(tx).await
// }

// pub async fn init_collections(
//     set: bool,
//     candy_machine: &Pubkey,
//     authority: Keypair,
//     sized: bool,
//     solana_client: Arc<RpcClient>,
// ) -> Self {
//     println!("Init Collection Info");
//     let mint = Keypair::new();
//     let mint_pubkey = mint.pubkey();
//     let program_id = mpl_token_metadata::id();

//     let metadata_seeds = &[PREFIX.as_bytes(), program_id.as_ref(), mint_pubkey.as_ref()];
//     let (pubkey, _) = Pubkey::find_program_address(metadata_seeds, &program_id);

//     let collection_details = if sized {
//         Some(CollectionDetails::V1 { size: 0 })
//     } else {
//         None
//     };

//     let tx = Transaction::new_signed_with_payer(
//         &[instruction::create_metadata_accounts_v3(
//             mpl_token_metadata::id(),
//             self.pubkey,
//             self.mint.pubkey(),
//             self.authority.pubkey(),
//             self.authority.pubkey(),
//             self.authority.pubkey(),
//             "Collection Name".to_string(),
//             "COLLECTION".to_string(),
//             "URI".to_string(),
//             None,
//             0,
//             true,
//             true,
//             None,
//             None,
//             collection_details,
//         )],
//         Some(&self.authority.pubkey()),
//         &[&self.authority],
//         solana_client.get_latest_blockhash()?,
//     );
//     let master_edition_info = MasterEditionManager::new(&metadata_info);
//     master_edition_info
//         .create_v3(context, Some(0))
//         .await
//         .unwrap();

//     let collection_pda = find_collection_pda(candy_machine).0;
//     let collection_authority_record =
//         find_collection_authority_account(&metadata_info.mint.pubkey(), &collection_pda).0;

//     CollectionInfo {
//         set,
//         pda: collection_pda,
//         mint: clone_keypair(&metadata_info.mint),
//         metadata: metadata_info.pubkey,
//         master_edition: master_edition_info.edition_pubkey,
//         token_account: metadata_info.get_ata(),
//         authority_record: collection_authority_record,
//         sized,
//     }
// }

// pub async fn get_metadata(&self, context: &mut ProgramTestContext) -> Metadata {
//     MetadataManager::get_data_from_account(context, &self.metadata).await
// }
