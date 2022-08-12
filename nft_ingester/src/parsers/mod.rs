mod bubblegum;
mod gummyroll;

pub use self::bubblegum::*;
pub use self::gummyroll::*;

use {
    crate::{error::IngesterError, utils::IxPair},
    async_trait::async_trait,
    flatbuffers::{ForwardsUOffset, Vector},
    plerkle_serialization::{
        account_info_generated::account_info,
        transaction_info_generated::transaction_info::{self, CompiledInstruction},
    },
    solana_sdk::pubkey::Pubkey,
    std::collections::HashMap,
};

pub struct ProgramHandlerManager<'a> {
    registered_parsers: HashMap<Pubkey, Box<dyn ProgramHandler + 'a>>,
}

impl<'a> ProgramHandlerManager<'a> {
    pub fn new() -> Self {
        ProgramHandlerManager {
            registered_parsers: HashMap::new(),
        }
    }

    pub fn register_parser(&mut self, parser: Box<dyn ProgramHandler + 'a>) {
        let id = parser.id();
        self.registered_parsers.insert(id, parser);
    }

    pub fn match_program(&self, program_id: &[u8]) -> Option<&dyn ProgramHandler> {
        self.registered_parsers
            .get(&Pubkey::new(program_id))
            .map(|parser| parser.as_ref())
    }
}

pub struct ProgramHandlerConfig {
    pub responds_to_account: bool,
    pub responds_to_instruction: bool,
}

pub struct InstructionBundle<'a, 'b> {
    pub message_id: i64,
    pub txn_id: String,
    pub instruction: CompiledInstruction<'a>,
    pub inner_ix: Option<Vec<IxPair<'a>>>,
    pub keys: Vector<'b, ForwardsUOffset<transaction_info::Pubkey<'b>>>,
    pub instruction_logs: Vec<&'b str>,
    pub slot: u64,
}

/// A abtraction over handling program updates, account
#[async_trait]
pub trait ProgramHandler: Sync + Send {
    fn id(&self) -> Pubkey;

    fn config(&self) -> &ProgramHandlerConfig;

    async fn handle_instruction(&self, _bundle: &InstructionBundle) -> Result<(), IngesterError> {
        Ok(())
    }

    async fn handle_account(
        &self,
        _account_info: &account_info::AccountInfo,
    ) -> Result<(), IngesterError> {
        Ok(())
    }
}
