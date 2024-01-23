use crate::dao::sea_orm_active_enums::Instruction;

impl From<&str> for Instruction {
    fn from(s: &str) -> Self {
        match s {
            "Burn" => Instruction::Burn,
            "CancelRedeem" => Instruction::CancelRedeem,
            "Compress" => Instruction::Compress,
            "DecompressV1" => Instruction::DecompressV1,
            "Delegate" => Instruction::Delegate,
            "MintToCollectionV1" => Instruction::MintToCollectionV1,
            "MintV1" => Instruction::MintV1,
            "Redeem" => Instruction::Redeem,
            "SetAndVerifyCollection" => Instruction::SetAndVerifyCollection,
            "Transfer" => Instruction::Transfer,
            "UnverifyCollection" => Instruction::UnverifyCollection,
            "UnverifyCreator" => Instruction::UnverifyCreator,
            "VerifyCollection" => Instruction::VerifyCollection,
            "VerifyCreator" => Instruction::VerifyCreator,
            "UpdateMetadata" => Instruction::UpdateMetadata,
            _ => Instruction::Unknown,
        }
    }
}

pub trait PascalCase {
    fn to_pascal_case(&self) -> String;
}

impl PascalCase for Instruction {
    fn to_pascal_case(&self) -> String {
        let s = match self {
            Instruction::Burn => "Burn",
            Instruction::CancelRedeem => "CancelRedeem",
            Instruction::Compress => "Compress",
            Instruction::DecompressV1 => "DecompressV1",
            Instruction::Delegate => "Delegate",
            Instruction::MintToCollectionV1 => "MintToCollectionV1",
            Instruction::MintV1 => "MintV1",
            Instruction::Redeem => "Redeem",
            Instruction::SetAndVerifyCollection => "SetAndVerifyCollection",
            Instruction::Transfer => "Transfer",
            Instruction::Unknown => "Unknown",
            Instruction::UnverifyCollection => "UnverifyCollection",
            Instruction::UnverifyCreator => "UnverifyCreator",
            Instruction::VerifyCollection => "VerifyCollection",
            Instruction::VerifyCreator => "VerifyCreator",
            Instruction::UpdateMetadata => "UpdateMetadata",
        };
        s.to_string()
    }
}
