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
            "BurnV2" => Instruction::BurnV2,
            "DelegateV2" => Instruction::DelegateV2,
            "DelegateAndFreezeV2" => Instruction::DelegateAndFreezeV2,
            "FreezeV2" => Instruction::FreezeV2,
            "MintV2" => Instruction::MintV2,
            "SetCollectionV2" => Instruction::SetCollectionV2,
            "SetNonTransferableV2" => Instruction::SetNonTransferableV2,
            "ThawV2" => Instruction::ThawV2,
            "ThawAndRevokeV2" => Instruction::ThawAndRevokeV2,
            "TransferV2" => Instruction::TransferV2,
            "UnverifyCreatorV2" => Instruction::UnverifyCreatorV2,
            "VerifyCreatorV2" => Instruction::VerifyCreatorV2,
            "UpdateMetadataV2" => Instruction::UpdateMetadataV2,
            "UpdateAssetDataV2" => Instruction::UpdateAssetDataV2,
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
            Instruction::BurnV2 => "BurnV2",
            Instruction::DelegateV2 => "DelegateV2",
            Instruction::DelegateAndFreezeV2 => "DelegateAndFreezeV2",
            Instruction::FreezeV2 => "FreezeV2",
            Instruction::MintV2 => "MintV2",
            Instruction::SetCollectionV2 => "SetCollectionV2",
            Instruction::SetNonTransferableV2 => "SetNonTransferableV2",
            Instruction::ThawV2 => "ThawV2",
            Instruction::ThawAndRevokeV2 => "ThawAndRevokeV2",
            Instruction::TransferV2 => "TransferV2",
            Instruction::UnverifyCreatorV2 => "UnverifyCreatorV2",
            Instruction::VerifyCreatorV2 => "VerifyCreatorV2",
            Instruction::UpdateMetadataV2 => "UpdateMetadataV2",
            Instruction::UpdateAssetDataV2 => "UpdateAssetDataV2",
        };
        s.to_string()
    }
}
