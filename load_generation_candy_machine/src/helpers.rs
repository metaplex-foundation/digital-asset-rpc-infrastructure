use mpl_candy_machine::CollectionPDA;
use mpl_candy_machine_core::constants::AUTHORITY_SEED;
use mpl_token_metadata::{
    instruction,
    state::{Collection, CollectionDetails, Creator, Uses},
};
use solana_client::{client_error::ClientError, nonblocking::rpc_client::RpcClient};
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    program_pack::Pack, signature::Keypair, signer::Signer, system_instruction,
    transaction::Transaction,
};
use spl_token::state::Mint;
use std::sync::Arc;

pub fn find_authority_pda(candy_machine_key: &Pubkey) -> (Pubkey, u8) {
    let authority_seeds = [AUTHORITY_SEED.as_bytes(), candy_machine_key.as_ref()];
    Pubkey::find_program_address(&authority_seeds, &mpl_candy_machine_core::id())
}

pub fn find_collection_pda(candy_machine_key: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &["collection".as_bytes(), candy_machine_key.as_ref()],
        &mpl_candy_machine_core::id(),
    )
}

pub async fn create_mint(
    authority: &Pubkey,
    freeze_authority: Option<&Pubkey>,
    decimals: u8,
    mint: Arc<Keypair>,
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
) -> Result<(), ClientError> {
    let tx = Transaction::new_signed_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                solana_client
                    .get_minimum_balance_for_rent_exemption(Mint::LEN)
                    .await?,
                Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint.pubkey(),
                authority,
                freeze_authority,
                decimals,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
        &[payer.as_ref(), mint.as_ref()],
        solana_client.get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await?;

    Ok(())
}

pub async fn create_v3(
    name: String,
    symbol: String,
    uri: String,
    creators: Option<Vec<Creator>>,
    seller_fee_basis_points: u16,
    is_mutable: bool,
    freeze_authority: Option<&Pubkey>,
    collection: Option<Collection>,
    uses: Option<Uses>,
    sized: bool,
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    mint: Arc<Keypair>,
    metadata_account: Pubkey,
) -> Result<(), ClientError> {
    create_mint(
        &payer.pubkey(),
        freeze_authority,
        0,
        mint.clone(),
        solana_client.clone(),
        payer.clone(),
    )
    .await?;

    mint_to_wallets(
        &mint.clone().pubkey(),
        &payer.clone(),
        vec![(payer.pubkey(), 1)],
        payer.clone(),
        solana_client.clone(),
    )
    .await?;

    let collection_details = if sized {
        Some(CollectionDetails::V1 { size: 0 })
    } else {
        None
    };

    let tx = Transaction::new_signed_with_payer(
        &[instruction::create_metadata_accounts_v3(
            mpl_token_metadata::id(),
            metadata_account,
            mint.pubkey(),
            payer.pubkey(),
            payer.pubkey(),
            payer.pubkey(),
            name,
            symbol,
            uri,
            creators,
            seller_fee_basis_points,
            false,
            is_mutable,
            collection,
            uses,
            collection_details,
        )],
        Some(&payer.pubkey()),
        &[payer.as_ref()],
        solana_client.as_ref().get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await?;

    Ok(())
}

pub async fn create_associated_token_account(
    wallet: &Pubkey,
    token_mint: &Pubkey,
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
) -> Result<Pubkey, ClientError> {
    let tx = Transaction::new_signed_with_payer(
        &[
            spl_associated_token_account::instruction::create_associated_token_account(
                &payer.pubkey(),
                wallet,
                token_mint,
            ),
        ],
        Some(&payer.pubkey()),
        &[payer.as_ref()],
        solana_client.get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await?;

    Ok(spl_associated_token_account::get_associated_token_address(
        wallet, token_mint,
    ))
}

pub async fn mint_to_wallets(
    mint_pubkey: &Pubkey,
    authority: &Arc<Keypair>,
    allocations: Vec<(Pubkey, u64)>,
    payer: Arc<Keypair>,
    solana_client: Arc<RpcClient>,
) -> Result<Vec<Pubkey>, ClientError> {
    let mut atas = Vec::with_capacity(allocations.len());

    #[allow(clippy::needless_range_loop)]
    for i in 0..allocations.len() {
        let ata = create_associated_token_account(
            &allocations[i].0,
            mint_pubkey,
            solana_client.clone(),
            payer.clone(),
        )
        .await?;
        mint_tokens(
            authority,
            mint_pubkey,
            &ata,
            allocations[i].1,
            None,
            &payer,
            solana_client.clone(),
        )
        .await?;
        atas.push(ata);
    }
    Ok(atas)
}

pub async fn mint_tokens(
    authority: &Keypair,
    mint: &Pubkey,
    account: &Pubkey,
    amount: u64,
    additional_signer: Option<&Keypair>,
    payer: &Arc<Keypair>,
    solana_client: Arc<RpcClient>,
) -> Result<(), ClientError> {
    let mut signing_keypairs = vec![authority, &payer];
    if let Some(signer) = additional_signer {
        signing_keypairs.push(signer);
    }

    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        account,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &signing_keypairs,
        solana_client.get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await?;

    Ok(())
}

pub async fn create_v3_master_edition(
    max_supply: Option<u64>,
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    edition: Pubkey,
    mint: Pubkey,
    metadata_account: Pubkey,
) -> Result<(), ClientError> {
    let tx = Transaction::new_signed_with_payer(
        &[instruction::create_master_edition_v3(
            mpl_token_metadata::id(),
            edition,
            mint,
            payer.pubkey(),
            payer.pubkey(),
            metadata_account,
            payer.pubkey(),
            max_supply,
        )],
        Some(&payer.pubkey()),
        &[payer.as_ref()],
        solana_client.get_latest_blockhash().await?,
    );

    solana_client.send_and_confirm_transaction(&tx).await?;

    Ok(())
}
