use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use solana_zk_token_sdk::zk_token_elgamal::pod::{AeCiphertext, ElGamalCiphertext, ElGamalPubkey};
use spl_pod::{
    optional_keys::{OptionalNonZeroElGamalPubkey, OptionalNonZeroPubkey},
    primitives::{PodBool, PodI64, PodU16, PodU32, PodU64},
};

use spl_token_2022::extension::{
    confidential_transfer::{ConfidentialTransferAccount, ConfidentialTransferMint},
    confidential_transfer_fee::{ConfidentialTransferFeeAmount, ConfidentialTransferFeeConfig},
    cpi_guard::CpiGuard,
    default_account_state::DefaultAccountState,
    group_member_pointer::GroupMemberPointer,
    group_pointer::GroupPointer,
    interest_bearing_mint::{BasisPoints, InterestBearingConfig},
    memo_transfer::MemoTransfer,
    metadata_pointer::MetadataPointer,
    mint_close_authority::MintCloseAuthority,
    permanent_delegate::PermanentDelegate,
    transfer_fee::{TransferFee, TransferFeeAmount, TransferFeeConfig},
    transfer_hook::TransferHook,
};

use spl_token_group_interface::state::{TokenGroup, TokenGroupMember};
use spl_token_metadata_interface::state::TokenMetadata;

type PodAccountState = u8;
pub type UnixTimestamp = PodI64;

/// Bs58 encoded public key string. Used for storing Pubkeys in a human readable format.
/// Ideally we'd store them as is in the DB and later convert them to bs58 for display on the API.
/// But,
/// - We currently store them in DB as JSONB.
/// - `Pubkey` serializes to an u8 vector, unlike sth like `OptionalNonZeroElGamalPubkey` which serializes to a string.
///    So `Pubkey` is stored as a u8 vector in the DB.
/// - `Pubkey` doesn't implement something like `schemars::JsonSchema` so we can't convert them back to the rust struct either.
type PublicKeyString = String;

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowCpiGuard {
    pub lock_cpi: PodBool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowDefaultAccountState {
    pub state: PodAccountState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowInterestBearingConfig {
    pub rate_authority: OptionalNonZeroPubkey,
    pub initialization_timestamp: UnixTimestamp,
    pub pre_update_average_rate: BasisPoints,
    pub last_update_timestamp: UnixTimestamp,
    pub current_rate: BasisPoints,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowMemoTransfer {
    /// Require transfers into this account to be accompanied by a memo
    pub require_incoming_transfer_memos: PodBool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowMetadataPointer {
    pub metadata_address: OptionalNonZeroPubkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowGroupMemberPointer {
    pub member_address: OptionalNonZeroPubkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowGroupPointer {
    /// Account address that holds the group
    pub group_address: OptionalNonZeroPubkey,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShadowTokenGroup {
    /// The authority that can sign to update the group
    pub update_authority: OptionalNonZeroPubkey,
    /// The associated mint, used to counter spoofing to be sure that group
    /// belongs to a particular mint
    pub mint: PublicKeyString,
    /// The current number of group members
    pub size: PodU32,
    /// The maximum number of group members
    pub max_size: PodU32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShadowTokenGroupMember {
    /// The associated mint, used to counter spoofing to be sure that member
    /// belongs to a particular mint
    pub mint: PublicKeyString,
    /// The pubkey of the `TokenGroup`
    pub group: PublicKeyString,
    /// The member number
    pub member_number: PodU32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowMintCloseAuthority {
    pub close_authority: OptionalNonZeroPubkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct NonTransferableAccount;

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowPermanentDelegate {
    pub delegate: OptionalNonZeroPubkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowTransferFee {
    pub epoch: PodU64,
    pub maximum_fee: PodU64,
    pub transfer_fee_basis_points: PodU16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowTransferHook {
    pub authority: OptionalNonZeroPubkey,
    pub program_id: OptionalNonZeroPubkey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowConfidentialTransferMint {
    pub authority: OptionalNonZeroPubkey,
    pub auto_approve_new_accounts: PodBool,
    pub auditor_elgamal_pubkey: OptionalNonZeroElGamalPubkey,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShadowConfidentialTransferAccount {
    pub approved: PodBool,
    pub elgamal_pubkey: String,
    pub pending_balance_lo: String,
    pub pending_balance_hi: String,
    pub available_balance: String,
    pub decryptable_available_balance: String,
    pub allow_confidential_credits: PodBool,
    pub allow_non_confidential_credits: PodBool,
    pub pending_balance_credit_counter: PodU64,
    pub maximum_pending_balance_credit_counter: PodU64,
    pub expected_pending_balance_credit_counter: PodU64,
    pub actual_pending_balance_credit_counter: PodU64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShadowConfidentialTransferFeeConfig {
    pub authority: OptionalNonZeroPubkey,
    pub withdraw_withheld_authority_elgamal_pubkey: String,
    pub harvest_to_mint_enabled: PodBool,
    pub withheld_amount: String,
}

pub struct ShadowConfidentialTransferFeeAmount {
    pub withheld_amount: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowTransferFeeAmount {
    pub withheld_amount: PodU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Zeroable, Serialize, Deserialize)]
pub struct ShadowTransferFeeConfig {
    pub transfer_fee_config_authority: OptionalNonZeroPubkey,
    pub withdraw_withheld_authority: OptionalNonZeroPubkey,
    pub withheld_amount: PodU64,
    pub older_transfer_fee: ShadowTransferFee,
    pub newer_transfer_fee: ShadowTransferFee,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShadowMetadata {
    pub update_authority: OptionalNonZeroPubkey,
    pub mint: PublicKeyString,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub additional_metadata: Vec<(String, String)>,
}

impl From<CpiGuard> for ShadowCpiGuard {
    fn from(original: CpiGuard) -> Self {
        ShadowCpiGuard {
            lock_cpi: original.lock_cpi,
        }
    }
}

impl From<DefaultAccountState> for ShadowDefaultAccountState {
    fn from(original: DefaultAccountState) -> Self {
        ShadowDefaultAccountState {
            state: original.state,
        }
    }
}

impl From<ConfidentialTransferFeeAmount> for ShadowConfidentialTransferFeeAmount {
    fn from(original: ConfidentialTransferFeeAmount) -> Self {
        ShadowConfidentialTransferFeeAmount {
            withheld_amount: original.withheld_amount.to_base58(),
        }
    }
}

impl From<TransferFeeAmount> for ShadowTransferFeeAmount {
    fn from(original: TransferFeeAmount) -> Self {
        ShadowTransferFeeAmount {
            withheld_amount: original.withheld_amount,
        }
    }
}

impl From<MemoTransfer> for ShadowMemoTransfer {
    fn from(original: MemoTransfer) -> Self {
        ShadowMemoTransfer {
            require_incoming_transfer_memos: original.require_incoming_transfer_memos,
        }
    }
}

impl From<MetadataPointer> for ShadowMetadataPointer {
    fn from(original: MetadataPointer) -> Self {
        ShadowMetadataPointer {
            metadata_address: original.metadata_address,
        }
    }
}

impl From<GroupPointer> for ShadowGroupPointer {
    fn from(original: GroupPointer) -> Self {
        ShadowGroupPointer {
            group_address: original.group_address,
        }
    }
}

impl From<TokenGroup> for ShadowTokenGroup {
    fn from(original: TokenGroup) -> Self {
        ShadowTokenGroup {
            update_authority: original.update_authority,
            mint: original.mint.to_string(),
            size: original.size,
            max_size: original.max_size,
        }
    }
}

impl From<TokenGroupMember> for ShadowTokenGroupMember {
    fn from(original: TokenGroupMember) -> Self {
        ShadowTokenGroupMember {
            mint: original.mint.to_string(),
            group: original.group.to_string(),
            member_number: original.member_number,
        }
    }
}

impl From<GroupMemberPointer> for ShadowGroupMemberPointer {
    fn from(original: GroupMemberPointer) -> Self {
        ShadowGroupMemberPointer {
            member_address: original.member_address,
        }
    }
}

impl From<TransferFee> for ShadowTransferFee {
    fn from(original: TransferFee) -> Self {
        ShadowTransferFee {
            epoch: original.epoch,
            maximum_fee: original.maximum_fee,
            transfer_fee_basis_points: original.transfer_fee_basis_points,
        }
    }
}

impl From<TransferFeeConfig> for ShadowTransferFeeConfig {
    fn from(original: TransferFeeConfig) -> Self {
        ShadowTransferFeeConfig {
            transfer_fee_config_authority: original.transfer_fee_config_authority,
            withdraw_withheld_authority: original.withdraw_withheld_authority,
            withheld_amount: original.withheld_amount,
            older_transfer_fee: ShadowTransferFee::from(original.older_transfer_fee),
            newer_transfer_fee: ShadowTransferFee::from(original.newer_transfer_fee),
        }
    }
}

impl From<InterestBearingConfig> for ShadowInterestBearingConfig {
    fn from(original: InterestBearingConfig) -> Self {
        ShadowInterestBearingConfig {
            rate_authority: original.rate_authority,
            initialization_timestamp: original.initialization_timestamp,
            pre_update_average_rate: original.pre_update_average_rate,
            last_update_timestamp: original.last_update_timestamp,
            current_rate: original.current_rate,
        }
    }
}

impl From<MintCloseAuthority> for ShadowMintCloseAuthority {
    fn from(original: MintCloseAuthority) -> Self {
        ShadowMintCloseAuthority {
            close_authority: original.close_authority,
        }
    }
}

impl From<PermanentDelegate> for ShadowPermanentDelegate {
    fn from(original: PermanentDelegate) -> Self {
        ShadowPermanentDelegate {
            delegate: original.delegate,
        }
    }
}

impl From<TransferHook> for ShadowTransferHook {
    fn from(original: TransferHook) -> Self {
        ShadowTransferHook {
            authority: original.authority,
            program_id: original.program_id,
        }
    }
}

impl From<ConfidentialTransferMint> for ShadowConfidentialTransferMint {
    fn from(original: ConfidentialTransferMint) -> Self {
        ShadowConfidentialTransferMint {
            authority: original.authority,
            auto_approve_new_accounts: original.auto_approve_new_accounts,
            auditor_elgamal_pubkey: original.auditor_elgamal_pubkey,
        }
    }
}

impl From<ConfidentialTransferAccount> for ShadowConfidentialTransferAccount {
    fn from(original: ConfidentialTransferAccount) -> Self {
        ShadowConfidentialTransferAccount {
            approved: original.approved,
            elgamal_pubkey: original.elgamal_pubkey.to_base58(),
            pending_balance_lo: original.pending_balance_lo.to_base58(),
            pending_balance_hi: original.pending_balance_hi.to_base58(),
            available_balance: original.available_balance.to_base58(),
            decryptable_available_balance: original.decryptable_available_balance.to_base58(),
            allow_confidential_credits: original.allow_confidential_credits,
            allow_non_confidential_credits: original.allow_non_confidential_credits,
            pending_balance_credit_counter: original.pending_balance_credit_counter,
            maximum_pending_balance_credit_counter: original.maximum_pending_balance_credit_counter,
            expected_pending_balance_credit_counter: original
                .expected_pending_balance_credit_counter,
            actual_pending_balance_credit_counter: original.actual_pending_balance_credit_counter,
        }
    }
}

impl From<ConfidentialTransferFeeConfig> for ShadowConfidentialTransferFeeConfig {
    fn from(original: ConfidentialTransferFeeConfig) -> Self {
        ShadowConfidentialTransferFeeConfig {
            authority: original.authority,
            withdraw_withheld_authority_elgamal_pubkey: original
                .withdraw_withheld_authority_elgamal_pubkey
                .to_base58(),
            harvest_to_mint_enabled: original.harvest_to_mint_enabled,
            withheld_amount: original.withheld_amount.to_base58(),
        }
    }
}

impl From<TokenMetadata> for ShadowMetadata {
    fn from(original: TokenMetadata) -> Self {
        ShadowMetadata {
            update_authority: original.update_authority,
            mint: bs58::encode(original.mint).into_string(),
            name: original.name,
            symbol: original.symbol,
            uri: original.uri,
            additional_metadata: original.additional_metadata,
        }
    }
}

trait FromBytesToBase58 {
    fn to_base58(&self) -> String;
}

impl FromBytesToBase58 for ElGamalPubkey {
    fn to_base58(&self) -> String {
        bs58::encode(self.0).into_string()
    }
}

impl FromBytesToBase58 for ElGamalCiphertext {
    fn to_base58(&self) -> String {
        bs58::encode(self.0).into_string()
    }
}

impl FromBytesToBase58 for AeCiphertext {
    fn to_base58(&self) -> String {
        bs58::encode(self.0).into_string()
    }
}
