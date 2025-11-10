//! Shared library for the GDA2025 Zipf's law analysis project.
//!
//! This crate provides common functionality used across all binary crates:
//! - Configuration management
//! - Database models and operations
//! - Job queue management
//! - File path utilities
//! - Logging infrastructure
//! - Shared error types

pub mod config;
pub mod db;
pub mod disk_monitor;
pub mod logging;
pub mod models;
pub mod paths;
pub mod queue;

// Re-export commonly used types
pub use config::Config;
pub use db::Database;
pub use disk_monitor::{DiskMonitor, DiskUsage, SpaceBreakdown};
pub use logging::LogConfig;
pub use models::*;
pub use paths::DataPaths;
pub use queue::JobQueue;

/// Common result type using anyhow::Error
pub type Result<T> = anyhow::Result<T>;
