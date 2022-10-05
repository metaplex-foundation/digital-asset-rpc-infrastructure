mod candy_machine;
mod candy_machine_constants;
mod helpers;

use anchor_client::{
    solana_sdk::{commitment_config::CommitmentConfig, signature::keypair::Keypair},
    Client, Cluster, Program,
};
use candy_machine::make_a_candy_machine;
pub use mpl_candy_machine::ID as CANDY_MACHINE_ID;
use solana_client::client_error::ClientError;
use solana_client::rpc_request::RpcError::RpcRequestError;
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_sdk::signature::{keypair_from_seed, Signer};
use std::{env, rc::Rc, sync::Arc, time::Duration};
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
    let kp = Arc::new(
        keypair_from_seed(sow_thy_seed.as_ref())
            .expect("Thy Keypair is not available, I humbly suggest you look for it."),
    );

    let rpc_url = le_blockchain_url.clone();
    let ws_url = rpc_url.replace("http", "ws");
    let cluster = Cluster::Custom(rpc_url, ws_url);

    let kp_bytes = Keypair::from_bytes(&kp.to_bytes()).unwrap();
    let signer = Rc::new(kp_bytes);
    let le_blockchain = Arc::new(Client::new_with_options(
        cluster,
        signer,
        CommitmentConfig::confirmed(),
    ));

    let program = Arc::new(le_blockchain.program(CANDY_MACHINE_ID));
    let semaphore = Arc::new(Semaphore::new(carnage));
    check_balance(kp.clone(), network != "mainnet", program).await;
    loop {
        let mut tasks = vec![];
        for _ in 0..carnage {
            let kp = kp.clone();
            let le_clone = le_blockchain.clone();
            let semaphore = semaphore.clone();
            // tasks.push(tokio::spawn(async move {
            //     let _permit = semaphore.acquire().await.unwrap(); //wait for le government to allow le action
            //                                                       // create a candy machine:
            //     sleep(Duration::from_millis(5000)).await;
            //     make_a_candy_machine(program, kp).await
            // }));

            // Start tasks
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let res = make_a_candy_machine(program, kp).await;
                res
            }));
        }
        // for task in tasks {
        //     match task.await.unwrap() {
        //         Ok(e) => {
        //             println!("Successfully created a candy machine");
        //             continue;
        //         }
        //         Err(e) => {
        //             println!("Error: {:?}", e);
        //             continue;
        //         }
        //     }
        // }
        check_balance(kp.clone(), network != "mainnet", program).await;
    }
}

pub async fn check_balance(
    payer: Arc<Keypair>,
    airdrop: bool,
    program: Arc<Program>,
) -> Result<(), ClientError> {
    let sol = program.rpc().get_balance(&payer.pubkey())?;
    if sol / LAMPORTS_PER_SOL < 1 {
        if airdrop {
            program
                .rpc()
                .request_airdrop(&payer.pubkey(), LAMPORTS_PER_SOL * 2)?;
        } else {
            return Err(ClientError::from(RpcRequestError(
                "Not Enough Sol".to_string(),
            )));
        }
    }
    Ok(())
}
