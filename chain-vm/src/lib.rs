//! Virtual machine and state execution engine
//!
//! This crate provides the execution environment for transactions,
//! account management, and state transitions.

pub mod account;
pub mod error;
pub mod executor;
pub mod gas;
pub mod state;

pub use account::{Account, AccountState};
pub use error::{VmError, VmResult};
pub use executor::{ExecutionResult, StateChange, TransactionExecutor};
pub use gas::{GasMeter, GasSchedule};
pub use state::{StateDB, StateSnapshot};

#[cfg(test)]
mod tests {
    #[test]
    fn test_vm_basics() {
        // Basic smoke test
        assert!(true);
    }
}
