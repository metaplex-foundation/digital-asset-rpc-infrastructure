pub const DEFAULT_UUID: &str = "ABCDEF";
pub const DEFAULT_PRICE: u64 = LAMPORTS_PER_SOL;
pub const ITEMS_AVAILABLE: u64 = 11;
pub const DEFAULT_SYMBOL: &str = "SYMBOL";

pub async fn candy_machine_things(solana_client: Arc<RpcClient>, payer: Arc<Keypair>) -> Result<(), ClientError> {
 loop {
        let mut tasks = vec![];
        for _ in (0..carnage) {
            let kp = kp.clone();
            let le_clone = le_blockchain.clone();
            let semaphore = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap(); //wait for le government to allow le action
                // MINT A MASTER EDITION:
                sleep(Duration::from_millis(1000)).await;
                make_a_candy_machine(le_clone, kp).await
                  sleep(Duration::from_millis(10000)).await;
            }));
        }
        for task in tasks {
            match task.await.unwrap() {
                Ok(e) => {
                    println!("Successfully minted a NFT");
                    continue;
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                    continue;
                }
            }
        }
        check_balance(le_blockchain.clone(), kp.clone(), network != "mainnet").await;
    }
}

pub async fn make_a_candy_machine(solana_client: Arc<RpcClient>, payer: Arc<Keypair>)-> Result<(), ClientError>{
    let candy_data =  CandyMachineData {
        uuid: DEFAULT_UUID.to_string(),
        items_available: 3,
        price: DEFAULT_PRICE,
        symbol: DEFAULT_SYMBOL.to_string(),
        seller_fee_basis_points: 500,
        max_supply: 0,
        creators: vec![Creator {
            address: creator,
            verified: true,
            share: 100,
        }],
        is_mutable,
        retain_authority,
        go_live_date,
        end_settings,
        hidden_settings,
        whitelist_mint_settings,
        gatekeeper,
    };

        let candy_account_size = if candy_data.hidden_settings.is_some() {
        CONFIG_ARRAY_START
    } else {
        CONFIG_ARRAY_START
            + 4
            + items_available as usize * CONFIG_LINE_SIZE
            + 8
            + 2 * (items_available as usize / 8 + 1)
    };

    let rent = solana_client.get_rent().await?;
    let lamports = rent.minimum_balance(candy_account_size);
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

    if token_info.set {
        accounts.push(AccountMeta::new_readonly(token_info.mint, false));
    }

    let data = mpl_candy_machine::instruction::InitializeCandyMachine { data: candy_data }.data();

    let init_ix = Instruction {
        program_id: mpl_candy_machine::id(),
        data,
        accounts,
    };

 let current_slot = solana_client.get_root_slot().await?;
    solana_client
        .warp_to_slot(current_slot + 5)
        .map_err(|_| TransportError::Custom("Warp to slot failed!".to_string()))?;    
        let tx = Transaction::new_signed_with_payer(
        &[create_ix, init_ix],
        Some(&payer.pubkey()),
        &[candy_account, payer],
        context.last_blockhash,
    );

     solana_client.send_and_confirm_transaction(&tx).await?;
         Ok(())
}