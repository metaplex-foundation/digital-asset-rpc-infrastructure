use std::sync::Arc;

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_client::{client_error::ClientError, nonblocking::rpc_client::RpcClient};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program, sysvar,
    transaction::Transaction,
};

// use mpl_candy_machine::WhitelistMintMode::BurnEveryTime;

pub async fn mint_nft(
    candy_machine: &Pubkey,
    candy_creator_pda: &Pubkey,
    creator_bump: u8,
    wallet: &Pubkey,
    authority: &Pubkey,
    payer: Arc<Keypair>,
    edition_pubkey: Pubkey,
    metadata_pubkey: Pubkey,
    mint: Arc<Keypair>,
    token_account: Pubkey,
    solana_client: Arc<RpcClient>,
    // token_info: TokenInfo,
    // whitelist_info: WhitelistInfo,
    // collection_info: CollectionInfo,
    // gateway_info: GatekeeperInfo,
    // freeze_info: FreezeInfo,
) -> Result<(), ClientError> {
    let ix = mint_nft_ix(
        candy_machine,
        candy_creator_pda,
        creator_bump,
        wallet,
        authority,
        payer.clone(),
        edition_pubkey,
        metadata_pubkey,
        mint,
        token_account,
        // token_info,
        // whitelist_info,
        // collection_info,
        // gateway_info,
        // freeze_info,
    );

    let signers = vec![payer.as_ref()];
    let tx = Transaction::new_signed_with_payer(
        &ix,
        Some(&payer.pubkey()),
        &signers,
        solana_client.get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await;

    Ok(())
}

pub fn mint_nft_ix(
    candy_machine: &Pubkey,
    candy_creator_pda: &Pubkey,
    creator_bump: u8,
    wallet: &Pubkey,
    authority: &Pubkey,
    payer: Arc<Keypair>,
    edition_pubkey: Pubkey,
    metadata_pubkey: Pubkey,
    mint: Arc<Keypair>,
    token_account: Pubkey,
    // token_info: TokenInfo,
    // whitelist_info: WhitelistInfo,
    // collection_info: CollectionInfo,
    // gateway_info: GatekeeperInfo,
    // freeze_info: FreezeInfo,
) -> Vec<Instruction> {
    let metadata = metadata_pubkey;
    let master_edition = edition_pubkey;
    let mint = mint.pubkey();

    let mut accounts = mpl_candy_machine::accounts::MintNFT {
        candy_machine: *candy_machine,
        candy_machine_creator: *candy_creator_pda,
        payer: payer.pubkey(),
        wallet: *wallet,
        metadata,
        mint,
        mint_authority: payer.pubkey(),
        update_authority: payer.pubkey(),
        master_edition,
        token_metadata_program: mpl_token_metadata::id(),
        token_program: spl_token::id(),
        system_program: system_program::id(),
        rent: sysvar::rent::id(),
        clock: sysvar::clock::id(),
        recent_blockhashes: sysvar::slot_hashes::id(),
        instruction_sysvar_account: sysvar::instructions::id(),
    }
    .to_account_metas(None);

    // if gateway_info.set {
    //     accounts.push(AccountMeta::new(gateway_info.gateway_token_info, false));

    //     if gateway_info.gatekeeper_config.expire_on_use {
    //         accounts.push(AccountMeta::new_readonly(gateway_info.gateway_app, false));
    //         if let Some(expire_token) = gateway_info.network_expire_feature {
    //             accounts.push(AccountMeta::new_readonly(expire_token, false));
    //         }
    //     }
    // }

    // if whitelist_info.set {
    //     accounts.push(AccountMeta::new(whitelist_info.minter_account, false));
    //     if whitelist_info.whitelist_config.burn == BurnEveryTime {
    //         accounts.push(AccountMeta::new(whitelist_info.mint, false));
    //         accounts.push(AccountMeta::new_readonly(payer.pubkey(), true));
    //     }
    // }

    // if token_info.set {
    //     accounts.push(AccountMeta::new(token_info.minter_account, false));
    //     accounts.push(AccountMeta::new_readonly(payer.pubkey(), false));
    // }

    // if freeze_info.set {
    //     accounts.push(AccountMeta::new(freeze_info.pda, false));
    //     accounts.push(AccountMeta::new(new_nft.token_account, false));
    //     if token_info.set {
    //         accounts.push(AccountMeta::new(
    //             freeze_info.find_freeze_ata(&token_info.mint),
    //             false,
    //         ));
    //     }
    // }

    let data = mpl_candy_machine::instruction::MintNft { creator_bump }.data();

    let mut instructions = Vec::new();

    let mint_ix = Instruction {
        program_id: mpl_candy_machine::id(),
        data,
        accounts,
    };
    instructions.push(mint_ix);

    // if collection_info.set {
    //     let mut accounts = mpl_candy_machine::accounts::SetCollectionDuringMint {
    //         candy_machine: *candy_machine,
    //         metadata,
    //         payer: payer.pubkey(),
    //         collection_pda: collection_info.pda,
    //         token_metadata_program: mpl_token_metadata::id(),
    //         instructions: sysvar::instructions::id(),
    //         collection_mint: collection_info.mint.pubkey(),
    //         collection_metadata: collection_info.metadata,
    //         collection_master_edition: collection_info.master_edition,
    //         authority: *authority,
    //         collection_authority_record: collection_info.authority_record,
    //     }
    //     .to_account_metas(None);

    // if collection_info.sized {
    //     accounts
    //         .iter_mut()
    //         .find(|m| m.pubkey == collection_info.metadata)
    //         .unwrap()
    //         .is_writable = true;
    // }
    //     let data = mpl_candy_machine::instruction::SetCollectionDuringMint {}.data();
    //     let set_ix = Instruction {
    //         program_id: mpl_candy_machine::id(),
    //         data,
    //         accounts,
    //     };
    //     instructions.push(set_ix)
    // }
    instructions
}
