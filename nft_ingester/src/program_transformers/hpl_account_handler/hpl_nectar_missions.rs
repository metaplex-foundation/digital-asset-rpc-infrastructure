use {anchor_lang::prelude::*, hpl_toolkit::prelude::*};

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct MissionPool {
    pub bump: u8,
    pub project: Pubkey,
    pub name: String,
    pub factions_merkle_root: [u8; 32],
    pub randomizer_round: u8,
    pub character_models: Vec<Pubkey>,
    pub guild_kits: Vec<u8>,
}

impl anchor_lang::Discriminator for MissionPool {
    const DISCRIMINATOR: [u8; 8] = [106, 55, 99, 194, 178, 110, 104, 188];
}

#[derive(AnchorSerialize, AnchorDeserialize, ToSchema)]
pub struct Mission {
    pub bump: u8,
    pub project: Pubkey,
    pub mission_pool: Pubkey,
    pub name: String,
    pub min_xp: u64,
    pub cost: Currency,
    pub requirement: MissionRequirement,
    pub rewards: Vec<Reward>,
}

impl anchor_lang::Discriminator for Mission {
    const DISCRIMINATOR: [u8; 8] = [170, 56, 116, 75, 24, 11, 109, 12];
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, ToSchema)]
pub struct Currency {
    pub amount: u64,
    pub address: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, ToSchema)]
pub enum MissionRequirement {
    Time { duration: u64 },
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, ToSchema)]
pub struct Reward {
    pub min: u64,
    pub max: u64,
    pub reward_type: RewardType,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, ToSchema)]
pub enum RewardType {
    Xp,
    Currency { address: Pubkey },
}
