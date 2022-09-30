use blockbuster::token_metadata::state::{TokenStandard, Uses};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ChainDataV1 {
    pub name: String,
    pub symbol: String,
    pub edition_nonce: Option<u8>,
    pub primary_sale_happened: bool,
    pub token_standard: Option<TokenStandard>,
    pub uses: Option<Uses>,
}
