use borsh::BorshDeserialize;

use solana_client::{nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    UiTransactionEncoding,
};
use plerkle_serialization::serializer::seralize_encoded_transaction_with_status;
use spl_account_compression::state::{
    merkle_tree_get_size, ConcurrentMerkleTreeHeader, CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1,
};
use mpl_bubblegum;
use std::str::FromStr;
use clap::{Parser, Arg, arg, Command, ArgAction};

#[derive(Parser)]
// #[command(next_line_help = true)]
struct Cli {
    #[arg(long)]
    key: String,
    #[command(subcommand)]
    action: CheckTree,
}
#[derive(clap::Subcommand, Clone)]
enum Action {
    CheckTree {
        #[arg(long)]
        key: String,
    },
}
// define struct for the clap arguments


#[tokio::main]
async fn main() {
    let matches = Command::new("tree-status")
       .version("0.1")
       .author("TritonOne")
       .about("Test state of the sprcified tree.")
       .next_line_help(true)
       .arg(arg!(-k --key <PUBKEY>).required(false).action(ArgAction::Set))
       // .arg(Arg::new("key")
       //      .long("key")
       //      // .short("k")
       //      .index(1)
       //      .required(false)
       //      .help("The pubkey of the tree to check"))
       .get_matches();
    let mut arg = matches.get_one::<String>("key").to_owned().unwrap();
    // let arg = matches.get_one::<String>("key").to_owned.unwrap_or("8wKvdzBu2kEG5T3maJBX8m2gLs4XFavXzCKiZcGVeS8T");
    println!("arg: {:?}", arg);
    // let test = arg.trim().unwrap();
    // println!("test: {:?}", test);
    let mut pubkey =Pubkey::try_from(arg).unwrap();
    println!("pubkey: {:?}", pubkey);
    let cli = Cli::parse();
    let default_pubkey = Pubkey::try_from("8wKvdzBu2kEG5T3maJBX8m2gLs4XFavXzCKiZcGVeS8T").unwrap();
    let client = RpcClient::new(String::from("https://index.rpcpool.com/a4d23a00546272efeba9843a4ae4"));
    let cmd = cli.action;
    match cmd {
        Action::CheckTree { key } => {
            println!("Validating state of the tree: {:?}", default_pubkey);
            // println!("The pubkey vaue is: {:?}", pubkey);
            println!("The key vaue is: {:?}", key);
            let seq = get_tree_latest_seq(Pubkey::try_from(default_pubkey).unwrap(), &client).await;
            // let seq = get_tree_latest_seq(Pubkey::from(pubkey).unwrap_or(default_pubkey), &client).await;
            println!("seq: {:?}", seq);
        }
    }
}

pub async fn get_tree_latest_seq(
    address: Pubkey,
    client: &RpcClient,
) -> Result<u64, String> {
    // get account info
    let account_info = client   
                .get_account_with_commitment(
                    &address,
                    CommitmentConfig::confirmed(),
                )
                .await
                .map_err(|e| e.to_string())?;

    if let Some(mut account) = account_info.value {
            let (mut header_bytes, rest) = account
                .data
                .split_at_mut(CONCURRENT_MERKLE_TREE_HEADER_SIZE_V1);
            let header: ConcurrentMerkleTreeHeader =
                ConcurrentMerkleTreeHeader::try_from_slice(&mut header_bytes)
                    .map_err(|e| e.to_string())?;

            let auth = Pubkey::find_program_address(&[address.as_ref()], &mpl_bubblegum::id()).0;

            let merkle_tree_size = merkle_tree_get_size(&header)
                    .map_err(|e| e.to_string())?; 
            let (tree_bytes, canopy_bytes) = rest.split_at_mut(merkle_tree_size);

            let seq_bytes = tree_bytes[0..8].try_into()
                    .map_err(|e: _| "Error parsing bytes")?; 

            let seq = u64::from_le_bytes(seq_bytes);
            Ok(seq)
    } else {
        Err("No account found".to_string())
    }

    // get signatures
    /*let sigs = client
                .get_signatures_for_address_with_config(
                    &address,
                    GetConfirmedSignaturesForAddress2Config {
                        before: None,
                        until: None,
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..GetConfirmedSignaturesForAddress2Config::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?;

    if sigs.is_empty() {
        return Ok(0);
    }

    let mut first_sig = None;
    for sig in sigs.iter() {
        if sig.confirmation_status.is_none() || sig.err.is_some() {
            continue;
        }
        // Break on the first non err and confirmed transaction
        first_sig = Some(sig);
        break;
    };

    if let Some(first_sig) = first_sig {
        let sig_str = Signature::from_str(&first_sig.signature).unwrap();
        println!("first sig: {}", sig_str);

        let tx = client
            .get_transaction_with_config(
                &sig_str,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await
            .unwrap();

        // Being a bit lazy and encoding this flatbuffers. Almost certainly we don't have to
        let fbb = flatbuffers::FlatBufferBuilder::new();
        let fbb = seralize_encoded_transaction_with_status(fbb, tx);

        match fbb {
            Ok(fb_tx) => {
                let bytes = fb_tx.finished_data();

                println!("tx: {:?}", bytes);
            },
            Err(e) => {
                println!("err: {:?}", e);
            }
        }
    }*/
   /*let tx = Signature::from_str(sig).unwrap();
        .get_transaction_with_config(
            &sig,
            solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        )
        .await
        .unwrap();
*/
}

/* 
pub async fn handle_transaction<'a>(
    &self,
    tx: &'a TransactionInfo<'a>,
) -> Result<(), IngesterError> {
    info!("Handling Transaction: {:?}", tx.signature());
    let instructions = self.break_transaction(&tx);
    let accounts = tx.account_keys().unwrap_or_default();
    let slot = tx.slot();
    let mut keys: Vec<FBPubkey> = Vec::with_capacity(accounts.len());
    for k in accounts.into_iter() {
        keys.push(*k);
    }
    let mut not_impl = 0;
    let ixlen = instructions.len();
    debug!("Instructions: {}", ixlen);
    let contains = instructions
        .iter()
        .filter(|(ib, _inner)| ib.0 .0.as_ref() == mpl_bubblegum::id().as_ref());
    debug!("Instructions bgum: {}", contains.count());
    for (outer_ix, inner_ix) in instructions {
        let (program, instruction) = outer_ix;
        let ix_accounts = instruction.accounts().unwrap().iter().collect::<Vec<_>>();
        let ix_account_len = ix_accounts.len();
        let max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
        if keys.len() < max {
            return Err(IngesterError::DeserializationError(
                "Missing Accounts in Serialized Ixn/Txn".to_string(),
            ));
        }
        let ix_accounts =
            ix_accounts
                .iter()
                .fold(Vec::with_capacity(ix_account_len), |mut acc, a| {
                    if let Some(key) = keys.get(*a as usize) {
                        acc.push(*key);
                    }
                    acc
                });
        let ix = InstructionBundle {
            txn_id: "",
            program,
            instruction: Some(instruction),
            inner_ix,
            keys: ix_accounts.as_slice(),
            slot,
        };

        if let Some(program) = self.match_program(&ix.program) {
            debug!("Found a ix for program: {:?}", program.key());
            let result = program.handle_instruction(&ix)?;
            let concrete = result.result_type();
            match concrete {
                ProgramParseResult::Bubblegum(parsing_result) => {
                    handle_bubblegum_instruction(
                        parsing_result,
                        &ix,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await?;
                }
                _ => {
                    not_impl += 1;
                }
            };
        }
    }

    if not_impl == ixlen {
        debug!("Not imple");
        return Err(IngesterError::NotImplemented);
    }
    Ok(())
}
*/
