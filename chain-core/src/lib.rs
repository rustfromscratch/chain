//! Core blockchain data structures and traits
//!
//! This crate provides the fundamental building blocks for the blockchain system:
//! - Basic types (Hash, Address, BlockNumber, etc.)
//! - Transaction and Block structures  
//! - Trie interface for state management
//! - Cryptographic utilities

pub mod block;
pub mod error;
pub mod transaction;
pub mod trie;
pub mod types;

// Re-export commonly used types
pub use block::*;
pub use error::*;
pub use transaction::*;
pub use trie::*;
pub use types::*;
