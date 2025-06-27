//! Column family definitions for the blockchain database
//!
//! This module defines the column families used to organize data
//! in the key-value store.

use std::collections::HashMap;

/// Column family names
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnFamily {
    /// Default column family for misc data
    Default,
    /// Block bodies storage (block_hash -> RLP(Block))
    Blocks,
    /// Block headers storage (block_hash -> RLP(Header))
    Headers,
    /// Transaction receipts storage (block_hash -> RLP([Receipt]))
    Receipts,
    /// State trie nodes (trie_node_hash -> raw node data)
    State,
    /// Block number to hash index (block_number -> block_hash)
    Indices,
}

impl ColumnFamily {
    /// Get the string name for this column family
    pub fn name(&self) -> &'static str {
        match self {
            ColumnFamily::Default => "default",
            ColumnFamily::Blocks => "blocks",
            ColumnFamily::Headers => "headers",
            ColumnFamily::Receipts => "receipts",
            ColumnFamily::State => "state",
            ColumnFamily::Indices => "indices",
        }
    }

    /// Get all column families
    pub fn all() -> &'static [ColumnFamily] {
        &[
            ColumnFamily::Default,
            ColumnFamily::Blocks,
            ColumnFamily::Headers,
            ColumnFamily::Receipts,
            ColumnFamily::State,
            ColumnFamily::Indices,
        ]
    }

    /// Get column family from name
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "default" => Some(ColumnFamily::Default),
            "blocks" => Some(ColumnFamily::Blocks),
            "headers" => Some(ColumnFamily::Headers),
            "receipts" => Some(ColumnFamily::Receipts),
            "state" => Some(ColumnFamily::State),
            "indices" => Some(ColumnFamily::Indices),
            _ => None,
        }
    }
}

impl std::fmt::Display for ColumnFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Column family configuration
#[derive(Debug, Clone)]
pub struct ColumnFamilyConfig {
    /// Block cache size in bytes
    pub block_cache_size: u64,
    /// Write buffer size in bytes
    pub write_buffer_size: u64,
    /// Max write buffer number
    pub max_write_buffer_number: u32,
    /// Target file size base
    pub target_file_size_base: u64,
    /// Max bytes for level base
    pub max_bytes_for_level_base: u64,
    /// Compression type
    pub compression_type: CompressionType,
}

#[derive(Debug, Clone, Copy)]
pub enum CompressionType {
    None,
    Snappy,
    Zlib,
    Lz4,
    Zstd,
}

impl Default for ColumnFamilyConfig {
    fn default() -> Self {
        Self {
            block_cache_size: 64 * 1024 * 1024, // 64MB
            write_buffer_size: 32 * 1024 * 1024, // 32MB
            max_write_buffer_number: 3,
            target_file_size_base: 64 * 1024 * 1024, // 64MB
            max_bytes_for_level_base: 256 * 1024 * 1024, // 256MB
            compression_type: CompressionType::Lz4,
        }
    }
}

/// Get optimized configurations for each column family
pub fn get_column_family_configs() -> HashMap<ColumnFamily, ColumnFamilyConfig> {
    let mut configs = HashMap::new();
    
    // Default config for most CFs
    let default_config = ColumnFamilyConfig::default();
    
    // Headers: smaller cache since headers are accessed frequently but small
    let headers_config = ColumnFamilyConfig {
        block_cache_size: 32 * 1024 * 1024, // 32MB
        write_buffer_size: 16 * 1024 * 1024, // 16MB
        compression_type: CompressionType::Lz4,
        ..default_config.clone()
    };
    
    // Blocks: larger cache for block bodies
    let blocks_config = ColumnFamilyConfig {
        block_cache_size: 128 * 1024 * 1024, // 128MB
        write_buffer_size: 64 * 1024 * 1024, // 64MB
        compression_type: CompressionType::Zstd,
        ..default_config.clone()
    };
    
    // State: optimized for trie node access patterns
    let state_config = ColumnFamilyConfig {
        block_cache_size: 256 * 1024 * 1024, // 256MB
        write_buffer_size: 64 * 1024 * 1024, // 64MB
        target_file_size_base: 32 * 1024 * 1024, // 32MB
        compression_type: CompressionType::Zstd,
        ..default_config.clone()
    };
    
    // Indices: small and fast
    let indices_config = ColumnFamilyConfig {
        block_cache_size: 16 * 1024 * 1024, // 16MB
        write_buffer_size: 8 * 1024 * 1024, // 8MB
        compression_type: CompressionType::Lz4,
        ..default_config.clone()
    };
    
    configs.insert(ColumnFamily::Default, default_config.clone());
    configs.insert(ColumnFamily::Headers, headers_config);
    configs.insert(ColumnFamily::Blocks, blocks_config);
    configs.insert(ColumnFamily::Receipts, default_config.clone());
    configs.insert(ColumnFamily::State, state_config);
    configs.insert(ColumnFamily::Indices, indices_config);
    
    configs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_family_names() {
        assert_eq!(ColumnFamily::Default.name(), "default");
        assert_eq!(ColumnFamily::Blocks.name(), "blocks");
        assert_eq!(ColumnFamily::Headers.name(), "headers");
        assert_eq!(ColumnFamily::Receipts.name(), "receipts");
        assert_eq!(ColumnFamily::State.name(), "state");
        assert_eq!(ColumnFamily::Indices.name(), "indices");
    }

    #[test]
    fn test_column_family_from_name() {
        assert_eq!(ColumnFamily::from_name("default"), Some(ColumnFamily::Default));
        assert_eq!(ColumnFamily::from_name("blocks"), Some(ColumnFamily::Blocks));
        assert_eq!(ColumnFamily::from_name("invalid"), None);
    }

    #[test]
    fn test_all_column_families() {
        let all = ColumnFamily::all();
        assert_eq!(all.len(), 6);
        assert!(all.contains(&ColumnFamily::Default));
        assert!(all.contains(&ColumnFamily::Blocks));
    }

    #[test]
    fn test_column_family_configs() {
        let configs = get_column_family_configs();
        assert_eq!(configs.len(), 6);
        assert!(configs.contains_key(&ColumnFamily::State));
        
        let state_config = &configs[&ColumnFamily::State];
        assert!(state_config.block_cache_size > 0);
    }
}
