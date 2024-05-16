use {
    anchor_lang::prelude::*,
    hpl_toolkit::{compression::ControlledMerkleTrees, schema::*},
};

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct AssemblerConfig {
    pub key: Pubkey,
    pub project: Pubkey,
    pub layers: Vec<CharacterLayer>,
}

impl anchor_lang::Discriminator for AssemblerConfig {
    const DISCRIMINATOR: [u8; 8] = [129, 188, 134, 114, 66, 149, 112, 94];
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct CharacterLayer {
    pub label: String,
    pub traits: HashMap<String, String>, // name, uri
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct CharacterModel {
    pub bump: u8,

    /// The project this character is associated with
    pub key: Pubkey,

    /// The project this character is associated with
    pub project: Pubkey,

    /// Where this character came from
    pub config: CharacterConfig,

    /// Character specific attributes
    pub attributes: Schema,

    /// Character merkle trees
    pub merkle_trees: ControlledMerkleTrees,
}

impl anchor_lang::Discriminator for CharacterModel {
    const DISCRIMINATOR: [u8; 8] = [48, 232, 95, 182, 18, 16, 71, 113];
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub enum CharacterConfig {
    Wrapped(Vec<NftWrapCriteria>),
    Assembled {
        assembler_config: Pubkey,
        name: String,
        symbol: String,
        description: String,
        creators: Vec<NftCreator>,
        seller_fee_basis_points: u16,
        collection_name: String,
    },
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub enum NftWrapCriteria {
    Collection(Pubkey),
    Creator(Pubkey),
    MerkleTree(Pubkey),
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct NftCreator {
    pub address: Pubkey,
    pub share: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct AssetCustody {
    pub bump: u8,

    /// Where this character came from
    pub wallet: Pubkey,

    pub character_model: Option<Pubkey>,
    pub source: Option<CharacterSource>,
    pub character: Option<CharacterAssetConfig>,
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub enum CharacterSource {
    Wrapped {
        mint: Pubkey,
        criteria: NftWrapCriteria,
        is_compressed: bool,
    },
    Assembled {
        hash: Pubkey,
        mint: Pubkey,
        image: String,
        attributes: Vec<(String, String)>, // label, name
    },
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct CharacterAssetConfig {
    pub tree: Pubkey,
    pub leaf: u32,
}
