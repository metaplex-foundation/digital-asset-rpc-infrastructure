use crate::error::ProgramTransformerError;
use spl_account_compression::{ConcurrentMerkleTree, ConcurrentMerkleTreeError, Node};
use spl_concurrent_merkle_tree::changelog::ChangeLog;
use std::slice::Iter;

/// Interface that abstract [ConcurrentMerkleTree]<DEPTH, BUF_SIZE>
/// regardless const generic parameters.
pub trait ITree {
    fn initialize(&mut self) -> Result<Node, ConcurrentMerkleTreeError>;
    fn append(&mut self, node: Node) -> Result<Node, ConcurrentMerkleTreeError>;
    fn active_index(&self) -> u64;
    fn change_logs(&self, ind: usize) -> Box<dyn IChangeLog>;
    fn sequence_number(&self) -> u64;
    fn get_root(&self) -> [u8; 32];
}

#[macro_export]
macro_rules! make_tree_impls {
  ( $( ($x:literal, $y:literal) ),* ) => {
    $(
        impl ITree for ConcurrentMerkleTree<$x,$y> {
            fn initialize(&mut self) -> Result<Node, ConcurrentMerkleTreeError> {
                self.initialize()
            }
            fn append(&mut self, node: Node) -> Result<Node, ConcurrentMerkleTreeError> {
                self.append(node)
            }
            fn active_index(&self) -> u64 {
                self.active_index
            }
            fn change_logs(&self, ind: usize) -> Box<dyn IChangeLog> {
                Box::new(self.change_logs[ind])
            }
            fn sequence_number(&self) -> u64 {
                self.sequence_number
            }
            fn get_root(&self) -> [u8; 32] {
                self.get_root()
            }
        }
    )*
  }
}

// Building implementations of ITree
// for all possible instances of ConcurrentMerkleTreeError.
make_tree_impls!(
    (3, 8),
    (5, 8),
    (6, 16),
    (7, 16),
    (8, 16),
    (9, 16),
    (10, 32),
    (11, 32),
    (12, 32),
    (13, 32),
    (14, 64),
    (14, 256),
    (14, 1024),
    (14, 2048),
    (15, 64),
    (16, 64),
    (17, 64),
    (18, 64),
    (19, 64),
    (20, 64),
    (20, 256),
    (20, 1024),
    (20, 2048),
    (24, 64),
    (24, 256),
    (24, 512),
    (24, 1024),
    (24, 2048),
    (26, 512),
    (26, 1024),
    (26, 2048),
    (30, 512),
    (30, 1024),
    (30, 2048)
);

/// An abstraction for [ChangeLog]
/// that abstracts over const generic parameter.
/// Similar to [ITree].
pub trait IChangeLog {
    fn index(&self) -> u32;
    fn root(&self) -> [u8; 32];
    fn path_iter(&self) -> Iter<Node>;
    fn path_slice(&self) -> &[Node];
    fn path_len(&self) -> u32;
}

#[macro_export]
macro_rules! make_changelog_impls {
  ( $( $x:literal ),* ) => {
    $(
        impl IChangeLog for ChangeLog<$x> {
            fn index(&self) -> u32 {
                self.index
            }

            fn root(&self) -> [u8; 32] {
                self.root
            }

            fn path_iter(&self) -> Iter<Node> {
                self.path.iter()
            }

            fn path_slice(&self) -> &[Node] {
                &self.path
            }

            fn path_len(&self) -> u32 {
                self.path.len() as u32
            }
        }
    )*
  }
}

#[macro_export]
macro_rules! make_tree_creator_funcs {
  ( $( ($x:literal, $y:literal) ),* ) => {
    $(
        paste::item! {
            #[inline(never)]
            fn [< make_concurrent_merkle_tree_ $x _ $y >]() -> Box<dyn ITree> {
                Box::new(ConcurrentMerkleTree::<$x,$y>::new())
            }
        }
    )*
  }
}

make_tree_creator_funcs!(
    (3, 8),
    (5, 8),
    (6, 16),
    (7, 16),
    (8, 16),
    (9, 16),
    (10, 32),
    (11, 32),
    (12, 32),
    (13, 32),
    (14, 64),
    (14, 256),
    (14, 1024),
    (14, 2048),
    (15, 64),
    (16, 64),
    (17, 64),
    (18, 64),
    (19, 64),
    (20, 64),
    (20, 256),
    (20, 1024),
    (20, 2048),
    (24, 64),
    (24, 256),
    (24, 512),
    (24, 1024),
    (24, 2048),
    (26, 512),
    (26, 1024),
    (26, 2048),
    (30, 512),
    (30, 1024),
    (30, 2048)
);

pub fn make_concurrent_merkle_tree(
    max_dapth: u32,
    max_buf_size: u32,
) -> Result<Box<dyn ITree>, ProgramTransformerError> {
    // Note: We do not create ConcurrentMerkleTree<A,B> object right inside of match statement
    // because of how Rust compiler reserves space for functions:
    // the total size of function in memory (i.e. frame size) is as big as total size of
    // all possible ConcurrentMerkleTree<A,B> objects created in all branches.
    // Because they are allocated on stack.
    // And since these objects are quite big (up to 2MB),
    // the total size of function becomes so big, it cannot fit in the thread stack.
    // This all applies only for debug build, for release the compiler is smart enough
    // to optimize this.
    // Though, we need the debug to not fail with the stack overflow,
    // that's why we had to move creation of an exact ConcurrentMerkleTree<A,B> objects
    // into separate function that return trait objects.
    match (max_dapth, max_buf_size) {
        (3, 8) => Ok(make_concurrent_merkle_tree_3_8()),
        (5, 8) => Ok(make_concurrent_merkle_tree_5_8()),
        (6, 16) => Ok(make_concurrent_merkle_tree_6_16()),
        (7, 16) => Ok(make_concurrent_merkle_tree_7_16()),
        (8, 16) => Ok(make_concurrent_merkle_tree_8_16()),
        (9, 16) => Ok(make_concurrent_merkle_tree_9_16()),
        (10, 32) => Ok(make_concurrent_merkle_tree_10_32()),
        (11, 32) => Ok(make_concurrent_merkle_tree_11_32()),
        (12, 32) => Ok(make_concurrent_merkle_tree_12_32()),
        (13, 32) => Ok(make_concurrent_merkle_tree_13_32()),
        (14, 64) => Ok(make_concurrent_merkle_tree_14_64()),
        (14, 256) => Ok(make_concurrent_merkle_tree_14_256()),
        (14, 1024) => Ok(make_concurrent_merkle_tree_14_1024()),
        (14, 2048) => Ok(make_concurrent_merkle_tree_14_2048()),
        (15, 64) => Ok(make_concurrent_merkle_tree_15_64()),
        (16, 64) => Ok(make_concurrent_merkle_tree_16_64()),
        (17, 64) => Ok(make_concurrent_merkle_tree_17_64()),
        (18, 64) => Ok(make_concurrent_merkle_tree_18_64()),
        (19, 64) => Ok(make_concurrent_merkle_tree_19_64()),
        (20, 64) => Ok(make_concurrent_merkle_tree_20_64()),
        (20, 256) => Ok(make_concurrent_merkle_tree_20_256()),
        (20, 1024) => Ok(make_concurrent_merkle_tree_20_1024()),
        (20, 2048) => Ok(make_concurrent_merkle_tree_20_2048()),
        (24, 64) => Ok(make_concurrent_merkle_tree_24_64()),
        (24, 256) => Ok(make_concurrent_merkle_tree_24_256()),
        (24, 512) => Ok(make_concurrent_merkle_tree_24_512()),
        (24, 1024) => Ok(make_concurrent_merkle_tree_24_1024()),
        (24, 2048) => Ok(make_concurrent_merkle_tree_24_2048()),
        (26, 512) => Ok(make_concurrent_merkle_tree_26_512()),
        (26, 1024) => Ok(make_concurrent_merkle_tree_26_1024()),
        (26, 2048) => Ok(make_concurrent_merkle_tree_26_2048()),
        (30, 512) => Ok(make_concurrent_merkle_tree_30_512()),
        (30, 1024) => Ok(make_concurrent_merkle_tree_30_1024()),
        (30, 2048) => Ok(make_concurrent_merkle_tree_30_2048()),
        (d, s) => Err(ProgramTransformerError::UnexpectedTreeSize(d, s)),
    }
}

make_changelog_impls!(3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 24, 26, 30);
