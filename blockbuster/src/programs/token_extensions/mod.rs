pub mod extension;
use crate::{
    error::BlockbusterError,
    program_handler::{ParseResult, ProgramParser},
    programs::ProgramParseResult,
};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, pubkeys};
use spl_token_2022::{
    extension::{
        confidential_transfer::{ConfidentialTransferAccount, ConfidentialTransferMint},
        confidential_transfer_fee::ConfidentialTransferFeeConfig,
        cpi_guard::CpiGuard,
        default_account_state::DefaultAccountState,
        group_member_pointer::GroupMemberPointer,
        group_pointer::GroupPointer,
        interest_bearing_mint::InterestBearingConfig,
        memo_transfer::MemoTransfer,
        metadata_pointer::MetadataPointer,
        mint_close_authority::MintCloseAuthority,
        permanent_delegate::PermanentDelegate,
        transfer_fee::{TransferFeeAmount, TransferFeeConfig},
        transfer_hook::TransferHook,
        BaseStateWithExtensions, StateWithExtensions,
    },
    state::{Account, Mint},
};
use spl_token_group_interface::state::{TokenGroup, TokenGroupMember};
use spl_token_metadata_interface::state::TokenMetadata;

use self::extension::{
    ShadowConfidentialTransferAccount, ShadowConfidentialTransferFeeConfig,
    ShadowConfidentialTransferMint, ShadowCpiGuard, ShadowDefaultAccountState,
    ShadowGroupMemberPointer, ShadowGroupPointer, ShadowInterestBearingConfig, ShadowMemoTransfer,
    ShadowMetadata, ShadowMetadataPointer, ShadowMintCloseAuthority, ShadowPermanentDelegate,
    ShadowTokenGroup, ShadowTokenGroupMember, ShadowTransferFeeAmount, ShadowTransferFeeConfig,
    ShadowTransferHook,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MintAccountExtensions {
    pub default_account_state: Option<ShadowDefaultAccountState>,
    pub confidential_transfer_mint: Option<ShadowConfidentialTransferMint>,
    pub confidential_transfer_account: Option<ShadowConfidentialTransferAccount>,
    pub confidential_transfer_fee_config: Option<ShadowConfidentialTransferFeeConfig>,
    pub interest_bearing_config: Option<ShadowInterestBearingConfig>,
    pub transfer_fee_config: Option<ShadowTransferFeeConfig>,
    pub mint_close_authority: Option<ShadowMintCloseAuthority>,
    pub permanent_delegate: Option<ShadowPermanentDelegate>,
    pub metadata_pointer: Option<ShadowMetadataPointer>,
    pub metadata: Option<ShadowMetadata>,
    pub transfer_hook: Option<ShadowTransferHook>,
    pub group_pointer: Option<ShadowGroupPointer>,
    pub token_group: Option<ShadowTokenGroup>,
    pub group_member_pointer: Option<ShadowGroupMemberPointer>,
    pub token_group_member: Option<ShadowTokenGroupMember>,
}

impl MintAccountExtensions {
    pub fn is_some(&self) -> bool {
        self.default_account_state.is_some()
            || self.confidential_transfer_mint.is_some()
            || self.confidential_transfer_account.is_some()
            || self.confidential_transfer_fee_config.is_some()
            || self.interest_bearing_config.is_some()
            || self.transfer_fee_config.is_some()
            || self.mint_close_authority.is_some()
            || self.permanent_delegate.is_some()
            || self.metadata_pointer.is_some()
            || self.metadata.is_some()
            || self.transfer_hook.is_some()
            || self.group_pointer.is_some()
            || self.token_group.is_some()
            || self.group_member_pointer.is_some()
            || self.token_group_member.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenAccountExtensions {
    pub confidential_transfer: Option<ShadowConfidentialTransferAccount>,
    pub cpi_guard: Option<ShadowCpiGuard>,
    pub memo_transfer: Option<ShadowMemoTransfer>,
    pub transfer_fee_amount: Option<ShadowTransferFeeAmount>,
}

impl TokenAccountExtensions {
    pub fn is_some(&self) -> bool {
        self.confidential_transfer.is_some()
            || self.cpi_guard.is_some()
            || self.memo_transfer.is_some()
            || self.transfer_fee_amount.is_some()
    }
}
#[derive(Debug, PartialEq)]
pub struct TokenAccount {
    pub account: Account,
    pub extensions: TokenAccountExtensions,
}

#[derive(Debug, PartialEq)]
pub struct MintAccount {
    pub account: Mint,
    pub extensions: MintAccountExtensions,
}

pubkeys!(
    token_program_id,
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
);

pub struct Token2022AccountParser;

#[allow(clippy::large_enum_variant)]
pub enum TokenExtensionsProgramAccount {
    TokenAccount(TokenAccount),
    MintAccount(MintAccount),
    EmptyAccount,
}

impl ParseResult for TokenExtensionsProgramAccount {
    fn result(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }
    fn result_type(&self) -> ProgramParseResult {
        ProgramParseResult::TokenExtensionsProgramAccount(self)
    }
}

impl ProgramParser for Token2022AccountParser {
    fn key(&self) -> Pubkey {
        token_program_id()
    }
    fn key_match(&self, key: &Pubkey) -> bool {
        key == &token_program_id()
    }
    fn handles_account_updates(&self) -> bool {
        true
    }

    fn handles_instructions(&self) -> bool {
        false
    }

    fn handle_account(
        &self,
        account_data: &[u8],
    ) -> Result<Box<dyn ParseResult + 'static>, BlockbusterError> {
        if account_data.is_empty() {
            return Ok(Box::new(TokenExtensionsProgramAccount::EmptyAccount));
        }

        let result: TokenExtensionsProgramAccount;

        if let Ok(account) = StateWithExtensions::<Account>::unpack(account_data) {
            let confidential_transfer = account
                .get_extension::<ConfidentialTransferAccount>()
                .ok()
                .copied();
            let cpi_guard = account.get_extension::<CpiGuard>().ok().copied();
            let memo_transfer = account.get_extension::<MemoTransfer>().ok().copied();
            let transfer_fee_amount = account.get_extension::<TransferFeeAmount>().ok().copied();

            // Create a structured account with extensions
            let structured_account = TokenAccount {
                account: account.base,
                extensions: TokenAccountExtensions {
                    confidential_transfer: confidential_transfer
                        .map(ShadowConfidentialTransferAccount::from),
                    cpi_guard: cpi_guard.map(ShadowCpiGuard::from),
                    memo_transfer: memo_transfer.map(ShadowMemoTransfer::from),
                    transfer_fee_amount: transfer_fee_amount.map(ShadowTransferFeeAmount::from),
                },
            };

            result = TokenExtensionsProgramAccount::TokenAccount(structured_account);
        } else if let Ok(mint) = StateWithExtensions::<Mint>::unpack(account_data) {
            let confidential_transfer_mint = mint
                .get_extension::<ConfidentialTransferMint>()
                .ok()
                .copied();
            let confidential_transfer_account = mint
                .get_extension::<ConfidentialTransferAccount>()
                .ok()
                .copied();
            let confidential_transfer_fee_config = mint
                .get_extension::<ConfidentialTransferFeeConfig>()
                .ok()
                .copied();
            let default_account_state = mint.get_extension::<DefaultAccountState>().ok().copied();
            let interest_bearing_config =
                mint.get_extension::<InterestBearingConfig>().ok().copied();
            let transfer_fee_config = mint.get_extension::<TransferFeeConfig>().ok().copied();
            let mint_close_authority = mint.get_extension::<MintCloseAuthority>().ok().copied();
            let permanent_delegate = mint.get_extension::<PermanentDelegate>().ok().copied();
            let metadata_pointer = mint.get_extension::<MetadataPointer>().ok().copied();
            let metadata = mint.get_variable_len_extension::<TokenMetadata>().ok();
            let group_pointer = mint.get_extension::<GroupPointer>().ok().copied();
            let token_group = mint.get_extension::<TokenGroup>().ok().copied();
            let group_member_pointer = mint.get_extension::<GroupMemberPointer>().ok().copied();
            let token_group_member = mint.get_extension::<TokenGroupMember>().ok().copied();
            let transfer_hook = mint.get_extension::<TransferHook>().ok().copied();

            let structured_mint = MintAccount {
                account: mint.base,
                extensions: MintAccountExtensions {
                    confidential_transfer_mint: confidential_transfer_mint
                        .map(ShadowConfidentialTransferMint::from),
                    confidential_transfer_account: confidential_transfer_account
                        .map(ShadowConfidentialTransferAccount::from),
                    confidential_transfer_fee_config: confidential_transfer_fee_config
                        .map(ShadowConfidentialTransferFeeConfig::from),
                    default_account_state: default_account_state
                        .map(ShadowDefaultAccountState::from),
                    interest_bearing_config: interest_bearing_config
                        .map(ShadowInterestBearingConfig::from),
                    transfer_fee_config: transfer_fee_config.map(ShadowTransferFeeConfig::from),
                    mint_close_authority: mint_close_authority.map(ShadowMintCloseAuthority::from),
                    permanent_delegate: permanent_delegate.map(ShadowPermanentDelegate::from),
                    metadata_pointer: metadata_pointer.map(ShadowMetadataPointer::from),
                    metadata: metadata.map(ShadowMetadata::from),
                    transfer_hook: transfer_hook.map(ShadowTransferHook::from),
                    group_pointer: group_pointer.map(ShadowGroupPointer::from),
                    token_group: token_group.map(ShadowTokenGroup::from),
                    group_member_pointer: group_member_pointer.map(ShadowGroupMemberPointer::from),
                    token_group_member: token_group_member.map(ShadowTokenGroupMember::from),
                },
            };
            result = TokenExtensionsProgramAccount::MintAccount(structured_mint);
        } else {
            return Err(BlockbusterError::InvalidDataLength);
        };

        Ok(Box::new(result))
    }
}

pub trait IsNonFungible {
    fn is_non_fungible(&self) -> bool;
}

impl IsNonFungible for Mint {
    fn is_non_fungible(&self) -> bool {
        self.decimals == 0 && self.supply == 1
    }
}
