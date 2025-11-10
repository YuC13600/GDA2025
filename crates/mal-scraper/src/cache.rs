//! Cache management for MAL metadata.
//!
//! Implements permanent caching of API responses to avoid redundant requests.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Cache manager for API responses
pub struct CacheManager {
    /// Root cache directory
    cache_dir: PathBuf,
    /// Whether caching is enabled
    enabled: bool,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(cache_dir: impl AsRef<Path>, enabled: bool) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        if enabled {
            std::fs::create_dir_all(&cache_dir)
                .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;
            info!(cache_dir = %cache_dir.display(), "Cache initialized");
        }

        Ok(Self { cache_dir, enabled })
    }

    /// Get a cached item if it exists
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        if !self.enabled {
            return Ok(None);
        }

        let path = self.cache_path(key);
        if !path.exists() {
            debug!(key = key, "Cache miss");
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read cache file: {}", path.display()))?;

        let data: T = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache file: {}", path.display()))?;

        debug!(key = key, "Cache hit");
        Ok(Some(data))
    }

    /// Store an item in the cache
    pub fn set<T: Serialize>(&self, key: &str, data: &T) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let path = self.cache_path(key);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create cache subdirectory: {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(data)
            .context("Failed to serialize cache data")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write cache file: {}", path.display()))?;

        debug!(key = key, path = %path.display(), "Cache stored");
        Ok(())
    }

    /// Check if a cache entry exists
    pub fn exists(&self, key: &str) -> bool {
        if !self.enabled {
            return false;
        }
        self.cache_path(key).exists()
    }

    /// Get the cache file path for a given key
    fn cache_path(&self, key: &str) -> PathBuf {
        // Sanitize key to create valid filename
        let safe_key = key
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
            .replace("__", "_");

        self.cache_dir.join(format!("{}.json", safe_key))
    }

    /// Clear all cache
    pub fn clear(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.cache_dir.exists() {
            std::fs::remove_dir_all(&self.cache_dir)
                .with_context(|| format!("Failed to remove cache directory: {}", self.cache_dir.display()))?;
            std::fs::create_dir_all(&self.cache_dir)
                .with_context(|| format!("Failed to recreate cache directory: {}", self.cache_dir.display()))?;
            info!("Cache cleared");
        }

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        if !self.enabled || !self.cache_dir.exists() {
            return Ok(CacheStats {
                total_files: 0,
                total_size_bytes: 0,
            });
        }

        let mut total_files = 0;
        let mut total_size_bytes = 0;

        for entry in std::fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().is_file() {
                total_files += 1;
                total_size_bytes += entry.metadata()?.len();
            }
        }

        Ok(CacheStats {
            total_files,
            total_size_bytes,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: u32,
        name: String,
    }

    #[test]
    fn test_cache_enabled() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = CacheManager::new(temp_dir.path(), true)?;

        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };

        // Store data
        cache.set("test_key", &data)?;

        // Retrieve data
        let retrieved: Option<TestData> = cache.get("test_key")?;
        assert_eq!(retrieved, Some(data));

        Ok(())
    }

    #[test]
    fn test_cache_disabled() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = CacheManager::new(temp_dir.path(), false)?;

        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };

        // Store should succeed but do nothing
        cache.set("test_key", &data)?;

        // Retrieve should always return None
        let retrieved: Option<TestData> = cache.get("test_key")?;
        assert_eq!(retrieved, None);

        Ok(())
    }

    #[test]
    fn test_cache_miss() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = CacheManager::new(temp_dir.path(), true)?;

        let retrieved: Option<TestData> = cache.get("nonexistent")?;
        assert_eq!(retrieved, None);

        Ok(())
    }

    #[test]
    fn test_cache_exists() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = CacheManager::new(temp_dir.path(), true)?;

        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };

        assert!(!cache.exists("test_key"));
        cache.set("test_key", &data)?;
        assert!(cache.exists("test_key"));

        Ok(())
    }

    #[test]
    fn test_cache_stats() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = CacheManager::new(temp_dir.path(), true)?;

        let stats = cache.stats()?;
        assert_eq!(stats.total_files, 0);

        let data = TestData {
            id: 1,
            name: "test".to_string(),
        };
        cache.set("test_key", &data)?;

        let stats = cache.stats()?;
        assert_eq!(stats.total_files, 1);
        assert!(stats.total_size_bytes > 0);

        Ok(())
    }
}
