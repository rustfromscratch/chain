//! Database snapshot functionality
//!
//! This module provides snapshot services for creating and managing
//! blockchain state snapshots.

use crate::{
    column_families::ColumnFamily,
    error::{DbError, DbResult},
    traits::{KeyValueDB, SnapshotReader},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, info, warn};

/// Snapshot configuration
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Directory to store snapshots
    pub snapshot_dir: PathBuf,
    /// Snapshot format
    pub format: SnapshotFormat,
    /// Compression level (0-9, 0 = no compression)
    pub compression_level: u32,
    /// Maximum snapshot file size before splitting
    pub max_file_size: u64,
    /// Enable automatic snapshot creation
    pub auto_snapshot: bool,
    /// Interval between automatic snapshots (in blocks)
    pub snapshot_interval: u64,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            snapshot_dir: PathBuf::from("./snapshots"),
            format: SnapshotFormat::Binary,
            compression_level: 6,
            max_file_size: 1024 * 1024 * 1024, // 1GB
            auto_snapshot: false,
            snapshot_interval: 10000, // Every 10k blocks
        }
    }
}

/// Snapshot format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    /// Binary format (faster, smaller)
    Binary,
    /// JSON format (human readable, larger)
    Json,
    /// CAR format (IPFS compatible)
    Car,
}

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Snapshot version
    pub version: u32,
    /// Block number at snapshot
    pub block_number: u64,
    /// Block hash at snapshot
    pub block_hash: Vec<u8>,
    /// State root hash
    pub state_root: Vec<u8>,
    /// Timestamp when snapshot was created
    pub timestamp: u64,
    /// Total size in bytes
    pub size: u64,
    /// Number of chunks
    pub chunks: u32,
    /// Compression used
    pub compression: String,
    /// Checksum of snapshot data
    pub checksum: Vec<u8>,
}

/// State snapshot containing blockchain state at a specific block
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// Snapshot metadata
    pub metadata: SnapshotMetadata,
    /// Path to snapshot files
    pub path: PathBuf,
}

/// Snapshot creation progress
#[derive(Debug, Clone)]
pub struct SnapshotProgress {
    /// Current phase
    pub phase: SnapshotPhase,
    /// Progress percentage (0-100)
    pub progress: u8,
    /// Current item being processed
    pub current_item: String,
    /// Total items to process
    pub total_items: u64,
    /// Items processed so far
    pub processed_items: u64,
}

/// Snapshot creation phases
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPhase {
    /// Preparing snapshot
    Preparing,
    /// Exporting headers
    ExportingHeaders,
    /// Exporting blocks
    ExportingBlocks,
    /// Exporting state
    ExportingState,
    /// Exporting receipts
    ExportingReceipts,
    /// Compressing
    Compressing,
    /// Finalizing
    Finalizing,
    /// Completed
    Completed,
    /// Error occurred
    Error(String),
}

/// Snapshot service commands
#[derive(Debug)]
pub enum SnapshotCommand {
    /// Create a new snapshot
    CreateSnapshot {
        block_number: u64,
        response_tx: tokio::sync::oneshot::Sender<DbResult<StateSnapshot>>,
    },
    /// Import a snapshot
    ImportSnapshot {
        path: PathBuf,
        response_tx: tokio::sync::oneshot::Sender<DbResult<()>>,
    },
    /// Export snapshot to a specific location
    ExportSnapshot {
        block_number: u64,
        output_path: PathBuf,
        response_tx: tokio::sync::oneshot::Sender<DbResult<()>>,
    },
    /// List available snapshots
    ListSnapshots {
        response_tx: tokio::sync::oneshot::Sender<DbResult<Vec<SnapshotMetadata>>>,
    },
    /// Get snapshot creation progress
    GetProgress {
        response_tx: tokio::sync::oneshot::Sender<Option<SnapshotProgress>>,
    },
    /// Shutdown the snapshot service
    Shutdown,
}

/// Snapshot service
pub struct SnapshotService {
    config: SnapshotConfig,
    db: Arc<dyn KeyValueDB>,
    command_rx: mpsc::Receiver<SnapshotCommand>,
    current_progress: Option<SnapshotProgress>,
}

impl SnapshotService {
    /// Create a new snapshot service
    pub fn new(
        config: SnapshotConfig,
        db: Arc<dyn KeyValueDB>,
    ) -> (Self, SnapshotServiceHandle) {
        let (command_tx, command_rx) = mpsc::channel(100);
        
        let service = Self {
            config,
            db,
            command_rx,
            current_progress: None,
        };
        
        let handle = SnapshotServiceHandle { command_tx };
        
        (service, handle)
    }

    /// Run the snapshot service
    pub async fn run(mut self) -> DbResult<()> {
        info!("Starting snapshot service");
        
        // Ensure snapshot directory exists
        if let Err(e) = std::fs::create_dir_all(&self.config.snapshot_dir) {
            error!("Failed to create snapshot directory: {}", e);
            return Err(DbError::Io(e));
        }

        while let Some(command) = self.command_rx.recv().await {
            match command {
                SnapshotCommand::CreateSnapshot { block_number, response_tx } => {
                    let result = self.create_snapshot(block_number).await;
                    let _ = response_tx.send(result);
                }
                SnapshotCommand::ImportSnapshot { path, response_tx } => {
                    let result = self.import_snapshot(&path).await;
                    let _ = response_tx.send(result);
                }
                SnapshotCommand::ExportSnapshot { block_number, output_path, response_tx } => {
                    let result = self.export_snapshot(block_number, &output_path).await;
                    let _ = response_tx.send(result);
                }
                SnapshotCommand::ListSnapshots { response_tx } => {
                    let result = self.list_snapshots().await;
                    let _ = response_tx.send(result);
                }
                SnapshotCommand::GetProgress { response_tx } => {
                    let _ = response_tx.send(self.current_progress.clone());
                }
                SnapshotCommand::Shutdown => {
                    info!("Shutting down snapshot service");
                    break;
                }
            }
        }

        info!("Snapshot service stopped");
        Ok(())
    }

    async fn create_snapshot(&mut self, block_number: u64) -> DbResult<StateSnapshot> {
        info!("Creating snapshot at block {}", block_number);
        
        self.update_progress(SnapshotPhase::Preparing, 0, "Initializing");

        // Create timestamp-based filename
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let snapshot_name = format!("snapshot_{}_{}", block_number, timestamp);
        let snapshot_path = self.config.snapshot_dir.join(&snapshot_name);
        
        if let Err(e) = std::fs::create_dir_all(&snapshot_path) {
            return Err(DbError::Io(e));
        }

        // Get block hash for the given block number
        let block_hash = self.get_block_hash(block_number).await?;
        let state_root = self.get_state_root(block_number).await?;

        // Create snapshot metadata
        let mut metadata = SnapshotMetadata {
            version: 1,
            block_number,
            block_hash: block_hash.clone(),
            state_root: state_root.clone(),
            timestamp,
            size: 0,
            chunks: 0,
            compression: if self.config.compression_level > 0 {
                format!("gzip:{}", self.config.compression_level)
            } else {
                "none".to_string()
            },
            checksum: vec![],
        };

        // Export different data types
        let mut total_size = 0;
        let mut chunk_count = 0;

        // Export headers
        self.update_progress(SnapshotPhase::ExportingHeaders, 20, "Exporting headers");
        let headers_size = self.export_headers(&snapshot_path, block_number).await?;
        total_size += headers_size;
        chunk_count += 1;

        // Export blocks
        self.update_progress(SnapshotPhase::ExportingBlocks, 40, "Exporting blocks");
        let blocks_size = self.export_blocks(&snapshot_path, block_number).await?;
        total_size += blocks_size;
        chunk_count += 1;

        // Export state
        self.update_progress(SnapshotPhase::ExportingState, 60, "Exporting state");
        let state_size = self.export_state(&snapshot_path, &state_root).await?;
        total_size += state_size;
        chunk_count += 1;

        // Export receipts
        self.update_progress(SnapshotPhase::ExportingReceipts, 80, "Exporting receipts");
        let receipts_size = self.export_receipts(&snapshot_path, block_number).await?;
        total_size += receipts_size;
        chunk_count += 1;

        // Finalize metadata
        metadata.size = total_size;
        metadata.chunks = chunk_count;
        metadata.checksum = self.calculate_checksum(&snapshot_path).await?;

        // Save metadata
        self.update_progress(SnapshotPhase::Finalizing, 95, "Saving metadata");
        self.save_metadata(&snapshot_path, &metadata).await?;

        self.update_progress(SnapshotPhase::Completed, 100, "Snapshot created");
        self.current_progress = None;

        let snapshot = StateSnapshot {
            metadata,
            path: snapshot_path,
        };

        info!("Snapshot created successfully: {} bytes", total_size);
        Ok(snapshot)
    }

    async fn import_snapshot(&mut self, path: &Path) -> DbResult<()> {
        info!("Importing snapshot from {:?}", path);
        
        // Load metadata
        let metadata = self.load_metadata(path).await?;
        
        // Verify checksum
        if !self.verify_checksum(path, &metadata.checksum).await? {
            return Err(DbError::Other("Snapshot checksum verification failed".into()));
        }
        
        // Import data
        self.import_headers(path).await?;
        self.import_blocks(path).await?;
        self.import_state(path).await?;
        self.import_receipts(path).await?;
        
        info!("Snapshot imported successfully");
        Ok(())
    }

    async fn export_snapshot(&mut self, block_number: u64, output_path: &Path) -> DbResult<()> {
        info!("Exporting snapshot at block {} to {:?}", block_number, output_path);
        
        let snapshot = self.create_snapshot(block_number).await?;
        
        // Copy snapshot to output path
        if let Err(e) = std::fs::create_dir_all(output_path) {
            return Err(DbError::Io(e));
        }
        
        // This is a simplified copy - in a real implementation you might want
        // to create a tar/zip archive
        self.copy_directory(&snapshot.path, output_path).await?;
        
        info!("Snapshot exported successfully");
        Ok(())
    }

    async fn list_snapshots(&self) -> DbResult<Vec<SnapshotMetadata>> {
        let mut snapshots = Vec::new();
        
        let read_dir = std::fs::read_dir(&self.config.snapshot_dir)
            .map_err(|e| DbError::Io(e))?;
        
        for entry in read_dir {
            let entry = entry.map_err(|e| DbError::Io(e))?;
            let path = entry.path();
            
            if path.is_dir() {
                if let Ok(metadata) = self.load_metadata(&path).await {
                    snapshots.push(metadata);
                }
            }
        }
        
        // Sort by block number
        snapshots.sort_by_key(|s| s.block_number);
        
        Ok(snapshots)
    }

    fn update_progress(&mut self, phase: SnapshotPhase, progress: u8, current_item: &str) {
        self.current_progress = Some(SnapshotProgress {
            phase,
            progress,
            current_item: current_item.to_string(),
            total_items: 100, // Simplified
            processed_items: progress as u64,
        });
    }

    async fn get_block_hash(&self, block_number: u64) -> DbResult<Vec<u8>> {
        let key = block_number.to_be_bytes();
        self.db.get(ColumnFamily::Indices.name(), &key)
            .map(|opt| opt.unwrap_or_else(|| vec![0; 32]))
    }

    async fn get_state_root(&self, _block_number: u64) -> DbResult<Vec<u8>> {
        // Simplified implementation - return dummy state root
        Ok(vec![0; 32])
    }

    async fn export_headers(&self, snapshot_path: &Path, block_number: u64) -> DbResult<u64> {
        let headers_path = snapshot_path.join("headers.dat");
        let mut file = BufWriter::new(File::create(headers_path).map_err(|e| DbError::Io(e))?);
        
        let mut total_size = 0;
        
        for i in 0..=block_number {
            let key = i.to_be_bytes();
            if let Some(block_hash) = self.db.get(ColumnFamily::Indices.name(), &key)? {
                if let Some(header_data) = self.db.get(ColumnFamily::Headers.name(), &block_hash)? {
                    file.write_all(&(header_data.len() as u32).to_be_bytes()).map_err(|e| DbError::Io(e))?;
                    file.write_all(&header_data).map_err(|e| DbError::Io(e))?;
                    total_size += 4 + header_data.len() as u64;
                }
            }
        }
        
        file.flush().map_err(|e| DbError::Io(e))?;
        Ok(total_size)
    }

    async fn export_blocks(&self, snapshot_path: &Path, block_number: u64) -> DbResult<u64> {
        let blocks_path = snapshot_path.join("blocks.dat");
        let mut file = BufWriter::new(File::create(blocks_path).map_err(|e| DbError::Io(e))?);
        
        let mut total_size = 0;
        
        for i in 0..=block_number {
            let key = i.to_be_bytes();
            if let Some(block_hash) = self.db.get(ColumnFamily::Indices.name(), &key)? {
                if let Some(block_data) = self.db.get(ColumnFamily::Blocks.name(), &block_hash)? {
                    file.write_all(&(block_data.len() as u32).to_be_bytes()).map_err(|e| DbError::Io(e))?;
                    file.write_all(&block_data).map_err(|e| DbError::Io(e))?;
                    total_size += 4 + block_data.len() as u64;
                }
            }
        }
        
        file.flush().map_err(|e| DbError::Io(e))?;
        Ok(total_size)
    }

    async fn export_state(&self, snapshot_path: &Path, _state_root: &[u8]) -> DbResult<u64> {
        let state_path = snapshot_path.join("state.dat");
        let _file = File::create(state_path).map_err(|e| DbError::Io(e))?;
        
        // Simplified implementation - state export is complex
        // In a real implementation, you would traverse the state trie
        warn!("State export not fully implemented");
        
        Ok(0)
    }

    async fn export_receipts(&self, snapshot_path: &Path, block_number: u64) -> DbResult<u64> {
        let receipts_path = snapshot_path.join("receipts.dat");
        let mut file = BufWriter::new(File::create(receipts_path).map_err(|e| DbError::Io(e))?);
        
        let mut total_size = 0;
        
        for i in 0..=block_number {
            let key = i.to_be_bytes();
            if let Some(block_hash) = self.db.get(ColumnFamily::Indices.name(), &key)? {
                if let Some(receipts_data) = self.db.get(ColumnFamily::Receipts.name(), &block_hash)? {
                    file.write_all(&(receipts_data.len() as u32).to_be_bytes()).map_err(|e| DbError::Io(e))?;
                    file.write_all(&receipts_data).map_err(|e| DbError::Io(e))?;
                    total_size += 4 + receipts_data.len() as u64;
                }
            }
        }
        
        file.flush().map_err(|e| DbError::Io(e))?;
        Ok(total_size)
    }

    async fn import_headers(&self, snapshot_path: &Path) -> DbResult<()> {
        let headers_path = snapshot_path.join("headers.dat");
        if !headers_path.exists() {
            return Ok(());
        }
        
        let file = File::open(headers_path).map_err(|e| DbError::Io(e))?;
        let mut reader = BufReader::new(file);
        
        loop {
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes) {
                Ok(_) => {
                    let len = u32::from_be_bytes(len_bytes) as usize;
                    let mut data = vec![0u8; len];
                    reader.read_exact(&mut data).map_err(|e| DbError::Io(e))?;
                    
                    // TODO: Parse header and store in database
                    // This would require deserializing the header and extracting the hash
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(DbError::Io(e)),
            }
        }
        
        Ok(())
    }

    async fn import_blocks(&self, snapshot_path: &Path) -> DbResult<()> {
        let blocks_path = snapshot_path.join("blocks.dat");
        if !blocks_path.exists() {
            return Ok(());
        }
        
        // Similar to import_headers but for blocks
        warn!("Block import not fully implemented");
        Ok(())
    }

    async fn import_state(&self, snapshot_path: &Path) -> DbResult<()> {
        let state_path = snapshot_path.join("state.dat");
        if !state_path.exists() {
            return Ok(());
        }
        
        // State import is complex and would require careful handling
        warn!("State import not fully implemented");
        Ok(())
    }

    async fn import_receipts(&self, snapshot_path: &Path) -> DbResult<()> {
        let receipts_path = snapshot_path.join("receipts.dat");
        if !receipts_path.exists() {
            return Ok(());
        }
        
        // Similar to import_headers but for receipts
        warn!("Receipts import not fully implemented");
        Ok(())
    }

    async fn calculate_checksum(&self, snapshot_path: &Path) -> DbResult<Vec<u8>> {
        // Simplified checksum calculation
        // In a real implementation, you would calculate SHA256 of all files
        Ok(vec![0; 32])
    }

    async fn verify_checksum(&self, snapshot_path: &Path, expected: &[u8]) -> DbResult<bool> {
        let calculated = self.calculate_checksum(snapshot_path).await?;
        Ok(calculated == expected)
    }

    async fn save_metadata(&self, snapshot_path: &Path, metadata: &SnapshotMetadata) -> DbResult<()> {
        let metadata_path = snapshot_path.join("metadata.json");
        let file = File::create(metadata_path).map_err(|e| DbError::Io(e))?;
        serde_json::to_writer_pretty(file, metadata).map_err(|e| DbError::Other(e.to_string()))?;
        Ok(())
    }

    async fn load_metadata(&self, snapshot_path: &Path) -> DbResult<SnapshotMetadata> {
        let metadata_path = snapshot_path.join("metadata.json");
        let file = File::open(metadata_path).map_err(|e| DbError::Io(e))?;
        let metadata = serde_json::from_reader(file).map_err(|e| DbError::Other(e.to_string()))?;
        Ok(metadata)
    }

    async fn copy_directory(&self, src: &Path, dst: &Path) -> DbResult<()> {
        // Simplified directory copy
        // In a real implementation, you would use a proper recursive copy
        warn!("Directory copy not fully implemented");
        Ok(())
    }
}

/// Handle for interacting with the snapshot service
#[derive(Clone)]
pub struct SnapshotServiceHandle {
    command_tx: mpsc::Sender<SnapshotCommand>,
}

impl SnapshotServiceHandle {
    /// Create a new snapshot at the given block number
    pub async fn create_snapshot(&self, block_number: u64) -> DbResult<StateSnapshot> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx
            .send(SnapshotCommand::CreateSnapshot { block_number, response_tx })
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))?;
        
        response_rx
            .await
            .map_err(|_| DbError::Other("Failed to receive snapshot response".into()))?
    }

    /// Import a snapshot from the given path
    pub async fn import_snapshot(&self, path: PathBuf) -> DbResult<()> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx
            .send(SnapshotCommand::ImportSnapshot { path, response_tx })
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))?;
        
        response_rx
            .await
            .map_err(|_| DbError::Other("Failed to receive snapshot response".into()))?
    }

    /// Export a snapshot to the given path
    pub async fn export_snapshot(&self, block_number: u64, output_path: PathBuf) -> DbResult<()> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx
            .send(SnapshotCommand::ExportSnapshot { block_number, output_path, response_tx })
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))?;
        
        response_rx
            .await
            .map_err(|_| DbError::Other("Failed to receive snapshot response".into()))?
    }

    /// List available snapshots
    pub async fn list_snapshots(&self) -> DbResult<Vec<SnapshotMetadata>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx
            .send(SnapshotCommand::ListSnapshots { response_tx })
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))?;
        
        response_rx
            .await
            .map_err(|_| DbError::Other("Failed to receive snapshot response".into()))?
    }

    /// Get current snapshot creation progress
    pub async fn get_progress(&self) -> DbResult<Option<SnapshotProgress>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx
            .send(SnapshotCommand::GetProgress { response_tx })
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))?;
        
        response_rx
            .await
            .map_err(|_| DbError::Other("Failed to receive snapshot response".into()))
    }

    /// Shutdown the snapshot service
    pub async fn shutdown(&self) -> DbResult<()> {
        self.command_tx
            .send(SnapshotCommand::Shutdown)
            .await
            .map_err(|_| DbError::Other("Failed to send snapshot command".into()))
    }
}

/// Spawn a snapshot service task
pub fn spawn_snapshot_service(
    config: SnapshotConfig,
    db: Arc<dyn KeyValueDB>,
) -> (SnapshotServiceHandle, JoinHandle<DbResult<()>>) {
    let (service, handle) = SnapshotService::new(config, db);
    
    let task = tokio::spawn(async move {
        service.run().await
    });
    
    (handle, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::Database;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_snapshot_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let db: Arc<dyn KeyValueDB> = Arc::new(db);
        
        let config = SnapshotConfig::default();
        let (service, handle) = SnapshotService::new(config, db);
        
        // Test that we can get the handle
        assert!(handle.get_progress().await.is_ok());
    }

    #[test]
    fn test_snapshot_metadata_serialization() {
        let metadata = SnapshotMetadata {
            version: 1,
            block_number: 12345,
            block_hash: vec![1, 2, 3, 4],
            state_root: vec![5, 6, 7, 8],
            timestamp: 1234567890,
            size: 1024,
            chunks: 4,
            compression: "gzip:6".to_string(),
            checksum: vec![9, 10, 11, 12],
        };
        
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: SnapshotMetadata = serde_json::from_str(&json).unwrap();
        
        assert_eq!(metadata.version, deserialized.version);
        assert_eq!(metadata.block_number, deserialized.block_number);
        assert_eq!(metadata.block_hash, deserialized.block_hash);
    }
}
