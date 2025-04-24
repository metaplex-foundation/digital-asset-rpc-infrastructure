use bubblegum::BubblegumInstruction;
use mpl_core_program::MplCoreAccountState;
use system::SystemProgramAccount;
use token_account::TokenProgramEntity;
use token_extensions::TokenExtensionsProgramEntity;
use token_inscriptions::TokenInscriptionAccount;
use token_metadata::TokenMetadataAccountState;

pub mod bubblegum;
pub mod mpl_core_program;
pub mod system;
pub mod token_account;
pub mod token_extensions;
pub mod token_inscriptions;
pub mod token_metadata;

// Note: `ProgramParseResult` used to contain the following variants that have been deprecated and
// removed from blockbuster since the `version-1.16` tag:
// CandyGuard(&'a CandyGuardAccountData),
// CandyMachine(&'a CandyMachineAccountData),
// CandyMachineCore(&'a CandyMachineCoreAccountData),
//
// Candy Machine V3 parsing was removed because Candy Guard (`mpl-candy-guard`) and
// Candy Machine Core (`mpl-candy-machine-core`) were dependent upon a specific Solana
// version (1.16), there was no Candy Machine parsing in DAS (`digital-asset-rpc-infrastructure`),
// and we wanted to use the Rust clients for Bubblegum and Token Metadata so that going forward we
// could more easily update blockbuster to new Solana versions.
//
// Candy Machine V2 (`mpl-candy-machine`) parsing was removed at the same time as V3 because even
// though it did not depend on the `mpl-candy-machine` crate, it was also not being used by DAS.
pub enum ProgramParseResult<'a> {
    Bubblegum(&'a BubblegumInstruction),
    MplCore(&'a MplCoreAccountState),
    TokenMetadata(&'a TokenMetadataAccountState),
    TokenProgramEntity(&'a TokenProgramEntity),
    TokenExtensionsProgramEntity(&'a TokenExtensionsProgramEntity),
    TokenInscriptionAccount(&'a TokenInscriptionAccount),
    SystemProgramAccount(&'a SystemProgramAccount),
    Unknown,
}
