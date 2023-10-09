use {
    mpl_token_metadata::state::Creator,
    solana_client::{
        client_error::ClientError, nonblocking::rpc_client::RpcClient,
        rpc_request::RpcError::RpcRequestError,
    },
    solana_program::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey},
    solana_sdk::{
        signature::{keypair_from_seed, Signer},
        signer::keypair::Keypair,
        system_instruction,
        transaction::Transaction,
    },
    spl_token::solana_program::program_pack::Pack,
    std::{env, sync::Arc, time::Duration},
    tokio::{sync::Semaphore, time::sleep},
};

#[tokio::main]
async fn main() {
    let sow_thy_seed = env::var("KEYPAIR_SEED").unwrap_or_else(|_| {
        "Cast your bread upon the waters, for you will find it after many days.".to_string()
    });
    let le_blockchain_url =
        env::var("RPC_URL").unwrap_or_else(|_| "http://solana:8899".to_string());
    let network = env::var("NETWORK").unwrap_or_else(|_| "local".to_string());
    let carnage = env::var("AMOUNT_OF_CHAOS").map(|chaos_str| chaos_str.parse::<usize>().expect("How can you mess that up? Okay okay, your AMOUNT OF CHAOS variable is super messed up.")).unwrap_or_else(|_| 64);
    let le_blockchain = Arc::new(RpcClient::new_with_timeout_and_commitment(
        le_blockchain_url,
        Duration::from_secs(45),
        solana_sdk::commitment_config::CommitmentConfig::confirmed(),
    ));
    let kp = Arc::new(
        keypair_from_seed(sow_thy_seed.as_ref())
            .expect("Thy Keypair is not available, I humbly suggest you look for it."),
    );
    let kp_new = Arc::new(Keypair::new());
    let semaphore = Arc::new(Semaphore::new(carnage));
    let _ = check_balance(le_blockchain.clone(), kp.clone(), network != "mainnet").await;
    let nft_collection_thing =
        make_a_nft_thing(le_blockchain.clone(), kp.clone(), kp.clone(), None)
            .await
            .unwrap();
    println!("NFT Collection Thing: {:?}", nft_collection_thing);
    loop {
        let mut tasks = vec![];
        for _ in 0..carnage {
            let kp = kp.clone();
            let kp_new = kp_new.clone();
            let le_clone = le_blockchain.clone();
            let semaphore = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap(); //wait for le government to allow le action
                                                                  // MINT A MASTER EDITION:
                sleep(Duration::from_millis(1000)).await;
                make_a_nft_thing(le_clone, kp, kp_new, Some(nft_collection_thing)).await
            }));
        }
        for task in tasks {
            match task.await.unwrap() {
                Ok(_) => {
                    println!("Lo! and Behold ! Successfully minted a NFT");
                    continue;
                }
                Err(e) => {
                    println!("Woe is me , an Error: {:?}", e);
                    continue;
                }
            }
        }
        let _ = check_balance(le_blockchain.clone(), kp.clone(), network != "mainnet").await;
    }
}

pub async fn check_balance(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    airdrop: bool,
) -> Result<(), ClientError> {
    let sol = solana_client.get_balance(&payer.pubkey()).await?;
    if sol / LAMPORTS_PER_SOL < 1 {
        if airdrop {
            solana_client
                .request_airdrop(&payer.pubkey(), LAMPORTS_PER_SOL)
                .await?;
        } else {
            return Err(ClientError::from(RpcRequestError(
                "Woe is me ! I mourn in sackcloth and ashes for , Not Enough Sol".to_string(),
            )));
        }
    }
    Ok(())
}

pub async fn make_a_token_thing(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    owner: Arc<Keypair>,
    mint_number: u64,
) -> Result<(Pubkey, Pubkey), ClientError> {
    let mint = Keypair::new();
    let ta_ix = spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        &owner.pubkey(),
        &mint.pubkey(),
        &spl_token::id(),
    );
    let ta = ta_ix.accounts[1].pubkey;
    let tx = Transaction::new_signed_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &mint.pubkey(),
                solana_client
                    .get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)
                    .await?,
                spl_token::state::Mint::LEN as u64,
                &spl_token::id(),
            ),
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &mint.pubkey(),
                &payer.pubkey(),
                Some(&payer.pubkey()),
                0,
            )
            .unwrap(),
            ta_ix,
            spl_token::instruction::mint_to(
                &spl_token::id(),
                &mint.pubkey(),
                &ta,
                &payer.pubkey(),
                &[],
                mint_number,
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
        &[&payer, &mint],
        solana_client.get_latest_blockhash().await?,
    );
    let _res = solana_client.send_and_confirm_transaction(&tx).await?;
    Ok((mint.pubkey(), ta))
}

pub async fn make_a_nft_thing(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    owner: Arc<Keypair>,
    collection_mint: Option<Pubkey>,
) -> Result<Pubkey, ClientError> {
    let (mint, _token_account) =
        make_a_token_thing(solana_client.clone(), payer.clone(), owner.clone(), 1).await?;
    let prg_uid = mpl_token_metadata::id();
    let _metadata_seeds = &[
        mpl_token_metadata::state::PREFIX.as_bytes(),
        prg_uid.to_bytes().as_ref(),
        mint.as_ref(),
    ];
    let (pubkey, _) = mpl_token_metadata::pda::find_metadata_account(&mint);
    let (edition_pubkey, _) = mpl_token_metadata::pda::find_master_edition_account(&mint);
    let tx = Transaction::new_signed_with_payer(
        &[
            mpl_token_metadata::instruction::create_metadata_accounts_v3(
                prg_uid,
                pubkey,
                mint,
                payer.pubkey(),
                payer.pubkey(),
                payer.pubkey(),
                "fake".to_string(),
                "fake".to_string(),
                "https://usd363wqbeq4xmuyddhbicmvm5yzegh4ulnsmp67jebxi6mqe45q.arweave.net/pIe_btAJIcuymBjOFAmVZ3GSGPyi2yY_30kDdHmQJzs".to_string(),
                Some(vec![Creator {
                    address: payer.pubkey(),
                    verified: true,
                    share: 100,
                }]),
                0,
                true,
                true,
                None,
                None,
                None,
            ),
            mpl_token_metadata::instruction::create_master_edition_v3(
                prg_uid,
                edition_pubkey,
                mint,
                payer.pubkey(),
                payer.pubkey(),
                pubkey,
                payer.pubkey(),
                Some(0),
            ),
        ],
        Some(&payer.pubkey()),
        &[payer.as_ref()],
        solana_client.get_latest_blockhash().await?,
    );
    solana_client.send_and_confirm_transaction(&tx).await?;
    let mut ix = vec![
        mpl_token_metadata::instruction::update_metadata_accounts_v2(
            prg_uid,
            pubkey,
            payer.pubkey(),
            None,
            None,
            None,
            Some(false),
        ),
    ];

    if let Some(collection_mint) = collection_mint {
        let (collection_metadata, _u8) =
            mpl_token_metadata::pda::find_metadata_account(&collection_mint);
        let (collection_master_edition, _u8) =
            mpl_token_metadata::pda::find_master_edition_account(&collection_mint);
        ix.push(mpl_token_metadata::instruction::set_and_verify_collection(
            prg_uid,
            pubkey,
            payer.pubkey(),
            payer.pubkey(),
            payer.pubkey(),
            collection_mint,
            collection_metadata,
            collection_master_edition,
            None,
        ));
    }
    let tx = Transaction::new_signed_with_payer(
        &ix,
        Some(&payer.pubkey()),
        &[payer.as_ref()],
        solana_client.get_latest_blockhash().await?,
    );
    solana_client.send_and_confirm_transaction(&tx).await?;
    Ok(mint)
}
