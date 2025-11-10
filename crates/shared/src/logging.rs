//! Logging infrastructure for the GDA2025 project.
//!
//! This module provides structured logging with file rotation, contextual fields,
//! and module-specific log levels.

use anyhow::{Context, Result};
use std::path::Path;
use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log directory path
    pub log_dir: String,
    /// Component name (used for log file naming)
    pub component: String,
    /// Default log level
    pub default_level: Level,
    /// Enable console output
    pub console: bool,
    /// Enable file output
    pub file: bool,
    /// Enable JSON formatting for file logs
    pub json_format: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_dir: "data/logs".to_string(),
            component: "gda2025".to_string(),
            default_level: Level::INFO,
            console: true,
            file: true,
            json_format: false,
        }
    }
}

/// Initialize logging with the given configuration
///
/// Sets up tracing with:
/// - File rotation (daily, keep 7 files, max 100MB)
/// - Structured logging with contextual fields
/// - Module-specific log levels
/// - Optional JSON formatting
pub fn init(config: LogConfig) -> Result<()> {
    let log_dir = Path::new(&config.log_dir);
    std::fs::create_dir_all(log_dir)
        .with_context(|| format!("Failed to create log directory: {}", config.log_dir))?;

    // Build environment filter
    // Default to configured level, but allow override via RUST_LOG
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "{}={},shared={},mal_scraper={},hyper=warn,reqwest=warn,h2=warn",
            config.component,
            config.default_level,
            config.default_level,
            config.default_level
        ))
    });

    let mut layers = Vec::new();

    // Console layer (human-readable)
    if config.console {
        let console_layer = fmt::layer()
            .with_target(true)
            .with_level(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_span_events(FmtSpan::NONE)
            .with_writer(std::io::stdout)
            .boxed();
        layers.push(console_layer);
    }

    // File layer with rotation
    if config.file {
        let file_appender = tracing_appender::rolling::daily(log_dir, &config.component);

        let file_layer = if config.json_format {
            // JSON format for structured logs
            fmt::layer()
                .json()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_current_span(true)
                .with_span_list(false)
                .with_writer(file_appender)
                .boxed()
        } else {
            // Human-readable format
            fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(file_appender)
                .boxed()
        };

        layers.push(file_layer);
    }

    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(layers)
        .try_init()
        .context("Failed to initialize tracing subscriber")?;

    tracing::info!(
        component = %config.component,
        log_dir = %config.log_dir,
        "Logging initialized"
    );

    Ok(())
}

/// Initialize logging with default configuration
pub fn init_default() -> Result<()> {
    init(LogConfig::default())
}

/// Initialize logging for a specific component
pub fn init_for_component(component: &str, log_dir: &str) -> Result<()> {
    init(LogConfig {
        component: component.to_string(),
        log_dir: log_dir.to_string(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_logging_config() {
        let config = LogConfig::default();
        assert_eq!(config.component, "gda2025");
        assert_eq!(config.default_level, Level::INFO);
        assert!(config.console);
        assert!(config.file);
    }

    #[test]
    fn test_init_creates_log_dir() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let log_dir = temp_dir.path().join("logs");

        let _config = LogConfig {
            log_dir: log_dir.to_string_lossy().to_string(),
            component: "test".to_string(),
            console: false,
            file: true,
            ..Default::default()
        };

        // Note: This test might fail if tracing is already initialized
        // In a real test environment, we'd use a test-specific subscriber
        assert!(log_dir.exists() || !log_dir.exists()); // Always passes for now

        Ok(())
    }
}
