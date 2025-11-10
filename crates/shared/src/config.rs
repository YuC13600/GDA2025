//! Configuration management for the GDA2025 project.
//!
//! This module handles loading and parsing configuration from TOML files,
//! with sensible defaults for all settings.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Data directory settings
    pub data: DataConfig,

    /// Database settings
    pub database: DatabaseConfig,

    /// Logging settings
    pub logging: LoggingConfig,

    /// MAL scraper settings
    pub mal_scraper: MalScraperConfig,

    /// Disk management settings
    #[serde(default)]
    pub disk_management: DiskManagementConfig,

    /// Anthropic API settings
    #[serde(default)]
    pub anthropic: AnthropicConfig,
}

/// Data directory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    /// Root data directory path
    pub root_dir: String,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database file path (relative to data directory or absolute)
    pub path: String,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log directory path (relative to data directory or absolute)
    pub log_dir: String,

    /// Default log level (trace, debug, info, warn, error)
    pub default_level: String,

    /// Enable console output
    pub console: bool,

    /// Enable file output
    pub file: bool,

    /// Enable JSON formatting for file logs
    pub json_format: bool,
}

/// MAL scraper configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalScraperConfig {
    /// Jikan API base URL
    pub base_url: String,

    /// Rate limiting settings
    pub rate_limit: RateLimitConfig,

    /// Cache settings
    pub cache: CacheConfig,

    /// Minimum items required to process a category
    pub min_category_items: usize,

    /// Maximum retries for failed requests
    pub max_retries: u32,

    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per second
    pub requests_per_second: f64,

    /// Maximum requests per minute
    pub requests_per_minute: u32,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,

    /// Cache directory (relative to data directory)
    pub cache_dir: String,

    /// Cache expiration in seconds (None = permanent)
    pub expiration_seconds: Option<u64>,
}

/// Disk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskManagementConfig {
    /// Hard limit in GB
    pub hard_limit_gb: u64,

    /// Pause downloads threshold in GB
    pub pause_threshold_gb: u64,

    /// Resume downloads threshold in GB
    pub resume_threshold_gb: u64,

    /// Check interval in seconds
    pub check_interval_seconds: u64,

    /// Cache duration for disk usage results in seconds
    pub cache_duration_seconds: u64,

    /// Maximum concurrent downloads
    pub max_concurrent_downloads: usize,

    /// Maximum concurrent transcriptions
    pub max_concurrent_transcriptions: usize,

    /// Cleanup configuration
    pub cleanup: CleanupConfig,
}

/// Cleanup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupConfig {
    /// Delete video after transcription
    pub delete_video_after_transcription: bool,

    /// Delete audio after transcription
    pub delete_audio_after_transcription: bool,

    /// Delete transcript after tokenization
    pub delete_transcript_after_tokenization: bool,

    /// Delete tokens after analysis
    pub delete_tokens_after_analysis: bool,
}

/// Anthropic API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Anthropic API key for Claude Haiku anime selection
    pub api_key: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
        }
    }
}

impl Default for DiskManagementConfig {
    fn default() -> Self {
        Self {
            hard_limit_gb: 250,
            pause_threshold_gb: 230,
            resume_threshold_gb: 200,
            check_interval_seconds: 30,
            cache_duration_seconds: 5,
            max_concurrent_downloads: 5,
            max_concurrent_transcriptions: 2,
            cleanup: CleanupConfig::default(),
        }
    }
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            delete_video_after_transcription: true,
            delete_audio_after_transcription: true,
            delete_transcript_after_tokenization: false,
            delete_tokens_after_analysis: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data: DataConfig {
                root_dir: "data".to_string(),
            },
            database: DatabaseConfig {
                path: "jobs.db".to_string(),
            },
            logging: LoggingConfig {
                log_dir: "logs".to_string(),
                default_level: "info".to_string(),
                console: true,
                file: true,
                json_format: false,
            },
            mal_scraper: MalScraperConfig {
                base_url: "https://api.jikan.moe/v4".to_string(),
                rate_limit: RateLimitConfig {
                    requests_per_second: 2.0,
                    requests_per_minute: 50,
                },
                cache: CacheConfig {
                    enabled: true,
                    cache_dir: "cache".to_string(),
                    expiration_seconds: None, // Permanent cache
                },
                min_category_items: 50,
                max_retries: 3,
                retry_delay_ms: 1000,
            },
            disk_management: DiskManagementConfig::default(),
            anthropic: AnthropicConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    ///
    /// If the file doesn't exist, returns the default configuration.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            tracing::warn!(
                path = %path.display(),
                "Config file not found, using defaults"
            );
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        tracing::info!(
            path = %path.display(),
            "Configuration loaded successfully"
        );

        Ok(config)
    }

    /// Load configuration from a TOML file or create default if not found
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        Self::from_file(path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to load config, using defaults");
            Self::default()
        })
    }

    /// Save configuration to a TOML file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize configuration")?;

        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        tracing::info!(
            path = %path.display(),
            "Configuration saved successfully"
        );

        Ok(())
    }

    /// Get the absolute path for the data directory
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.data.root_dir)
    }

    /// Get the absolute path for the database file
    pub fn database_path(&self) -> PathBuf {
        let db_path = Path::new(&self.database.path);
        if db_path.is_absolute() {
            db_path.to_path_buf()
        } else {
            self.data_dir().join(db_path)
        }
    }

    /// Get the absolute path for the log directory
    pub fn log_dir(&self) -> PathBuf {
        let log_path = Path::new(&self.logging.log_dir);
        if log_path.is_absolute() {
            log_path.to_path_buf()
        } else {
            self.data_dir().join(log_path)
        }
    }

    /// Get the absolute path for the cache directory
    pub fn cache_dir(&self) -> PathBuf {
        let cache_path = Path::new(&self.mal_scraper.cache.cache_dir);
        if cache_path.is_absolute() {
            cache_path.to_path_buf()
        } else {
            self.data_dir().join(cache_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.data.root_dir, "data");
        assert_eq!(config.database.path, "jobs.db");
        assert_eq!(config.mal_scraper.rate_limit.requests_per_second, 2.0);
        assert_eq!(config.mal_scraper.cache.expiration_seconds, None);
    }

    #[test]
    fn test_save_and_load_config() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("config.toml");

        let original_config = Config::default();
        original_config.save(&config_path)?;

        assert!(config_path.exists());

        let loaded_config = Config::from_file(&config_path)?;
        assert_eq!(loaded_config.data.root_dir, original_config.data.root_dir);
        assert_eq!(
            loaded_config.mal_scraper.base_url,
            original_config.mal_scraper.base_url
        );

        Ok(())
    }

    #[test]
    fn test_load_nonexistent_config() {
        let config = Config::from_file("nonexistent.toml").unwrap();
        // Should return default config without error
        assert_eq!(config.data.root_dir, "data");
    }

    #[test]
    fn test_path_resolution() {
        let config = Config::default();

        let db_path = config.database_path();
        assert!(db_path.ends_with("data/jobs.db"));

        let log_dir = config.log_dir();
        assert!(log_dir.ends_with("data/logs"));

        let cache_dir = config.cache_dir();
        assert!(cache_dir.ends_with("data/cache"));
    }
}
