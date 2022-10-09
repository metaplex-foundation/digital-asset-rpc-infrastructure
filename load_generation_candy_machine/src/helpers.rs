use mpl_candy_machine::CandyMachine;
use mpl_token_metadata::{
    instruction,
    state::{Collection, CollectionDetails, Creator, Metadata, Uses, PREFIX},
};
use solana_client::{client_error::ClientError, nonblocking::rpc_client::RpcClient};
use solana_program::pubkey::Pubkey;
use solana_sdk::{native_token::LAMPORTS_PER_SOL, signature::Keypair, signer::Signer};
use std::sync::Arc;

pub fn get_ata(owner: Keypair, mint: Keypair) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(&owner.pubkey(), &mint.pubkey())
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
