//! Database pruning and cleanup functionality
//!
//! This module provides pruning services to manage database size
//! by removing old data according to configured policies.

use crate::{
    column_families::ColumnFamily,
    error::{DbError, DbResult},
    traits::KeyValueDB,
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::mpsc, task::JoinHandle, time::interval};
use tracing::{debug, error, info, warn};

/// Pruning configuration
#[derive(Debug, Clone)]
pub struct PruningConfig {
    /// Pruning mode
    pub mode: PruningMode,
    /// Pruning interval
    pub interval: Duration,
    /// Enable automatic pruning
    pub enabled: bool,
    /// Retain blocks for light clients
    pub retain_blocks_light: u64,
    /// Retain blocks for full nodes
    pub retain_blocks_full: u64,
    /// State history pruning depth
    pub state_history_depth: u64,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            mode: PruningMode::Archive,
            interval: Duration::from_secs(3600), // 1 hour
            enabled: false,
            retain_blocks_light: 1024,
            retain_blocks_full: 100_000,
            state_history_depth: 128,
        }
    }
}

/// Pruning modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruningMode {
    /// Archive mode: keep all data
    Archive,
    /// Light mode: minimal data retention
    Light,
    /// Full mode: balanced data retention
    Full,
    /// Custom mode: user-defined retention
    Custom,
}

/// Pruning commands
#[derive(Debug)]
pub enum PruningCommand {
    /// Prune old blocks
    PruneBlocks { before_block: u64 },
    /// Prune old state
    PruneState { before_block: u64 },
    /// Prune receipts
    PruneReceipts { before_block: u64 },
    /// Compact database
    Compact,
    /// Get pruning statistics
    GetStats,
    /// Shutdown the pruner
    Shutdown,
}

/// Pruning statistics
#[derive(Debug, Clone)]
pub struct PruningStats {
    /// Last pruning time
    pub last_pruning: Option<SystemTime>,
    /// Number of blocks pruned
    pub blocks_pruned: u64,
    /// Number of state entries pruned
    pub state_entries_pruned: u64,
    /// Number of receipts pruned
    pub receipts_pruned: u64,
    /// Total bytes freed
    pub bytes_freed: u64,
    /// Pruning errors count
    pub errors: u64,
}

impl Default for PruningStats {
    fn default() -> Self {
        Self {
            last_pruning: None,
            blocks_pruned: 0,
            state_entries_pruned: 0,
            receipts_pruned: 0,
            bytes_freed: 0,
            errors: 0,
        }
    }
}

/// Database pruning service
pub struct Pruner {
    config: PruningConfig,
    db: Arc<dyn KeyValueDB>,
    stats: PruningStats,
    command_rx: mpsc::Receiver<PruningCommand>,
    command_tx: mpsc::Sender<PruningCommand>,
}

impl Pruner {
    /// Create a new pruner
    pub fn new(
        config: PruningConfig,
        db: Arc<dyn KeyValueDB>,
    ) -> (Self, PrunerHandle) {
        let (command_tx, command_rx) = mpsc::channel(100);
        
        let pruner = Self {
            config,
            db,
            stats: PruningStats::default(),
            command_rx,
            command_tx: command_tx.clone(),
        };
        
        let handle = PrunerHandle {
            command_tx,
        };
        
        (pruner, handle)
    }

    /// Start the pruning service
    pub async fn run(mut self) -> DbResult<()> {
        info!("Starting pruning service with mode: {:?}", self.config.mode);
        
        let mut interval_timer = if self.config.enabled {
            Some(interval(self.config.interval))
        } else {
            None
        };

        loop {
            tokio::select! {
                // Handle interval-based pruning
                _ = async {
                    if let Some(ref mut timer) = interval_timer {
                        timer.tick().await;
                    } else {
                        // If pruning is disabled, wait indefinitely
                        std::future::pending::<()>().await;
                    }
                } => {
                    if let Err(e) = self.perform_automatic_pruning().await {
                        error!("Automatic pruning failed: {}", e);
                        self.stats.errors += 1;
                    }
                }
                
                // Handle manual commands
                command = self.command_rx.recv() => {
                    match command {
                        Some(cmd) => {
                            if let Err(e) = self.handle_command(cmd).await {
                                error!("Pruning command failed: {}", e);
                                self.stats.errors += 1;
                            }
                        }
                        None => {
                            info!("Pruning service command channel closed");
                            break;
                        }
                    }
                }
            }
        }

        info!("Pruning service stopped");
        Ok(())
    }

    async fn perform_automatic_pruning(&mut self) -> DbResult<()> {
        debug!("Performing automatic pruning");
        
        let current_time = SystemTime::now();
        
        match self.config.mode {
            PruningMode::Archive => {
                // Archive mode: only compact, don't prune
                self.compact_database().await?;
            }
            PruningMode::Light => {
                // Light mode: aggressive pruning
                let retain_blocks = self.config.retain_blocks_light;
                self.prune_old_data(retain_blocks).await?;
            }
            PruningMode::Full => {
                // Full mode: balanced pruning
                let retain_blocks = self.config.retain_blocks_full;
                self.prune_old_data(retain_blocks).await?;
            }
            PruningMode::Custom => {
                // Custom mode: use configured settings
                let retain_blocks = self.config.retain_blocks_full;
                self.prune_old_data(retain_blocks).await?;
            }
        }
        
        self.stats.last_pruning = Some(current_time);
        info!("Automatic pruning completed");
        Ok(())
    }

    async fn handle_command(&mut self, command: PruningCommand) -> DbResult<()> {
        match command {
            PruningCommand::PruneBlocks { before_block } => {
                self.prune_blocks(before_block).await
            }
            PruningCommand::PruneState { before_block } => {
                self.prune_state(before_block).await
            }
            PruningCommand::PruneReceipts { before_block } => {
                self.prune_receipts(before_block).await
            }
            PruningCommand::Compact => {
                self.compact_database().await
            }
            PruningCommand::GetStats => {
                debug!("Pruning stats: {:?}", self.stats);
                Ok(())
            }
            PruningCommand::Shutdown => {
                info!("Shutting down pruning service");
                Ok(())
            }
        }
    }

    async fn prune_old_data(&mut self, retain_blocks: u64) -> DbResult<()> {
        // Get the current tip block number
        let tip_block = self.get_tip_block_number().await?;
        
        if tip_block <= retain_blocks {
            debug!("Not enough blocks to prune (tip: {}, retain: {})", tip_block, retain_blocks);
            return Ok(());
        }
        
        let prune_before = tip_block - retain_blocks;
        
        info!("Pruning data before block {}", prune_before);
        
        // Prune blocks, state, and receipts
        self.prune_blocks(prune_before).await?;
        self.prune_state(prune_before).await?;
        self.prune_receipts(prune_before).await?;
        
        // Compact after pruning
        self.compact_database().await?;
        
        Ok(())
    }

    async fn prune_blocks(&mut self, before_block: u64) -> DbResult<()> {
        debug!("Pruning blocks before {}", before_block);
        
        let mut pruned_count = 0;
        
        // Iterate through block indices and remove old blocks
        for block_num in 0..before_block {
            let key = block_num.to_be_bytes();
            
            // Get block hash from index
            if let Some(block_hash) = self.db.get(ColumnFamily::Indices.name(), &key)? {
                // Remove block body
                self.db.delete(ColumnFamily::Blocks.name(), &block_hash)?;
                
                // In light mode, also remove headers
                if self.config.mode == PruningMode::Light {
                    self.db.delete(ColumnFamily::Headers.name(), &block_hash)?;
                }
                
                // Remove index entry
                self.db.delete(ColumnFamily::Indices.name(), &key)?;
                
                pruned_count += 1;
            }
        }
        
        self.stats.blocks_pruned += pruned_count;
        info!("Pruned {} blocks", pruned_count);
        Ok(())
    }

    async fn prune_state(&mut self, before_block: u64) -> DbResult<()> {
        debug!("Pruning state before block {}", before_block);
        
        // This is a simplified implementation
        // In a real implementation, you would need to:
        // 1. Track which state nodes are referenced by which blocks
        // 2. Only remove state nodes that are not referenced by retained blocks
        // 3. Handle state trie pruning carefully to maintain consistency
        
        // For now, we'll just log that state pruning would happen here
        warn!("State pruning not fully implemented - would prune state before block {}", before_block);
        
        Ok(())
    }

    async fn prune_receipts(&mut self, before_block: u64) -> DbResult<()> {
        debug!("Pruning receipts before block {}", before_block);
        
        let mut pruned_count = 0;
        
        // Iterate through block indices and remove old receipts
        for block_num in 0..before_block {
            let key = block_num.to_be_bytes();
            
            // Get block hash from index
            if let Some(block_hash) = self.db.get(ColumnFamily::Indices.name(), &key)? {
                // Remove receipts
                self.db.delete(ColumnFamily::Receipts.name(), &block_hash)?;
                pruned_count += 1;
            }
        }
        
        self.stats.receipts_pruned += pruned_count;
        info!("Pruned {} receipt entries", pruned_count);
        Ok(())
    }

    async fn compact_database(&mut self) -> DbResult<()> {
        debug!("Compacting database");
        
        let start_time = SystemTime::now();
        
        // Compact all column families
        self.db.compact()?;
        
        let duration = start_time.elapsed().unwrap_or(Duration::from_secs(0));
        info!("Database compaction completed in {:?}", duration);
        
        Ok(())
    }

    async fn get_tip_block_number(&self) -> DbResult<u64> {
        // This is a simplified implementation
        // In a real implementation, you would store and retrieve the current tip
        // For now, return a dummy value
        Ok(1000)
    }
}

/// Handle for interacting with the pruning service
#[derive(Clone)]
pub struct PrunerHandle {
    command_tx: mpsc::Sender<PruningCommand>,
}

impl PrunerHandle {
    /// Manually trigger block pruning
    pub async fn prune_blocks(&self, before_block: u64) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::PruneBlocks { before_block })
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }

    /// Manually trigger state pruning
    pub async fn prune_state(&self, before_block: u64) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::PruneState { before_block })
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }

    /// Manually trigger receipts pruning
    pub async fn prune_receipts(&self, before_block: u64) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::PruneReceipts { before_block })
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }

    /// Manually trigger database compaction
    pub async fn compact(&self) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::Compact)
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }

    /// Get pruning statistics
    pub async fn get_stats(&self) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::GetStats)
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }

    /// Shutdown the pruning service
    pub async fn shutdown(&self) -> DbResult<()> {
        self.command_tx
            .send(PruningCommand::Shutdown)
            .await
            .map_err(|_| DbError::Other("Failed to send pruning command".into()))
    }
}

/// Spawn a pruning task
pub fn spawn_pruning_task(
    config: PruningConfig,
    db: Arc<dyn KeyValueDB>,
) -> (PrunerHandle, JoinHandle<DbResult<()>>) {
    let (pruner, handle) = Pruner::new(config, db);
    
    let task = tokio::spawn(async move {
        pruner.run().await
    });
    
    (handle, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::{Database, DatabaseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_pruner_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let db: Arc<dyn KeyValueDB> = Arc::new(db);
        
        let config = PruningConfig::default();
        let (pruner, handle) = Pruner::new(config, db);
        
        // Test that we can send commands
        handle.get_stats().await.unwrap();
    }

    #[tokio::test]
    async fn test_pruning_modes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let db: Arc<dyn KeyValueDB> = Arc::new(db);
        
        let mut config = PruningConfig::default();
        config.mode = PruningMode::Light;
        config.enabled = false; // Disable automatic pruning for test
        
        let (pruner, _handle) = Pruner::new(config, db);
        
        // Just test that the pruner can be created with different modes
        assert_eq!(pruner.config.mode, PruningMode::Light);
    }
}
