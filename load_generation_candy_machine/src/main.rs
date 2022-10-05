mod candy_machine;
mod candy_machine_constants;
mod helpers;

use candy_machine::make_a_candy_machine;
pub use mpl_candy_machine::ID as CANDY_MACHINE_ID;
use solana_client::rpc_request::RpcError::RpcRequestError;
use solana_client::{client_error::ClientError, rpc_client::RpcClient};
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::{keypair_from_seed, Keypair};
use solana_sdk::signer::Signer;
use std::{env, sync::Arc, time::Duration};
use tokio::{sync::Semaphore, time::sleep};

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
    let semaphore = Arc::new(Semaphore::new(carnage));
    check_balance(le_blockchain.clone(), kp.clone(), network != "mainnet").await;

    loop {
        let mut tasks = vec![];
        for _ in 0..carnage {
            let kp = kp.clone();
            let le_clone = le_blockchain.clone();
            let semaphore = semaphore.clone();

            // Start tasks
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                sleep(Duration::from_millis(5000)).await;
                let res = make_a_candy_machine(le_clone, kp).await;
                // TODO put the ids in a vec and then call update on them
                res
            }));
        }

        for task in tasks {
            match task.await.unwrap() {
                Ok(e) => {
                    println!("Candy machine created with an id of: {:?}", e);
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

pub async fn check_balance(
    solana_client: Arc<RpcClient>,
    payer: Arc<Keypair>,
    airdrop: bool,
) -> Result<(), ClientError> {
    let sol = solana_client.get_balance(&payer.pubkey())?;
    if sol / LAMPORTS_PER_SOL < 1 {
        if airdrop {
            solana_client.request_airdrop(&payer.pubkey(), LAMPORTS_PER_SOL)?;
        } else {
            return Err(ClientError::from(RpcRequestError(
                "Not Enough Sol".to_string(),
            )));
        }
    }
    Ok(())
}
