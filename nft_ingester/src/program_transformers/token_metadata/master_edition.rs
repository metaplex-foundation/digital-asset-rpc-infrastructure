use sea_orm::DatabaseTransaction;
use blockbuster::token_metadata::state::MasterEditionV2;
use crate::IngesterError;
use plerkle_serialization::Pubkey as FBPubkey;

pub async fn save_v2_master_edition<'c>(
    id: FBPubkey,
    slot: u64,
    metadata: &MasterEditionV2,
    txn: &'c DatabaseTransaction) -> Result<(), IngesterError> {


}


pub async fn save_v1_master_edition<'c>(
    id: FBPubkey,
    slot: u64,
    metadata: &MasterEditionV2,
    txn: &'c DatabaseTransaction) -> Result<(), IngesterError> {}
