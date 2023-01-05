mod full_asset;
mod generated;
pub mod scopes;
pub use full_asset::*;
pub use generated::*;

pub enum Pagination {
    Keyset {
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
    },
    Page {
        page: u64,
    },
}
