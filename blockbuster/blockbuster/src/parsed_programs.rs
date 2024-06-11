pub enum Program {
    Bubblegum {
        parser: bubblegum::BubblegumParser,
        instruction_result: BubblegumInstruction,
        account_result: (),
    },
}

impl ProgramParser for Program {}
