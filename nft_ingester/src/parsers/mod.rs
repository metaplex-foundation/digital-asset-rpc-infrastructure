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

