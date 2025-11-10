//! Disk space monitoring for coordinating downloads and transcriptions.
//!
//! This module provides utilities to monitor disk usage and determine when
//! to pause downloads to avoid exceeding storage limits.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Disk usage information.
#[derive(Debug, Clone)]
pub struct DiskUsage {
    /// Total bytes used by all files in data directory
    pub total_bytes: u64,
    /// Bytes used by video files
    pub videos_bytes: u64,
    /// Bytes used by audio files
    pub audio_bytes: u64,
    /// Bytes used by transcript files
    pub transcripts_bytes: u64,
    /// Bytes used by token files
    pub tokens_bytes: u64,
    /// Bytes used by cache
    pub cache_bytes: u64,
    /// Bytes used by database
    pub db_bytes: u64,
    /// Bytes used by other files
    pub other_bytes: u64,
}

impl DiskUsage {
    /// Calculate percentage of hard limit used.
    pub fn percentage(&self, hard_limit: u64) -> f64 {
        (self.total_bytes as f64 / hard_limit as f64) * 100.0
    }

    /// Get total bytes in GB.
    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1_000_000_000.0
    }

    /// Get temporary file usage (videos + audio).
    pub fn temporary_bytes(&self) -> u64 {
        self.videos_bytes + self.audio_bytes
    }

    /// Get permanent file usage (transcripts + tokens + cache + db).
    pub fn permanent_bytes(&self) -> u64 {
        self.transcripts_bytes + self.tokens_bytes + self.cache_bytes + self.db_bytes
    }
}

/// Detailed space breakdown with analysis.
#[derive(Debug, Clone)]
pub struct SpaceBreakdown {
    /// Current disk usage
    pub usage: DiskUsage,
    /// Percentage of hard limit used
    pub percentage: f64,
    /// Available bytes before hard limit
    pub available_bytes: u64,
    /// Whether downloads can proceed
    pub can_download: bool,
}

/// Cached disk usage result.
struct CachedUsage {
    usage: DiskUsage,
    timestamp: Instant,
}

/// Disk space monitor for coordinating pipeline components.
///
/// Monitors disk usage and provides pause/resume signals to prevent
/// exceeding storage limits. Uses caching to avoid excessive filesystem I/O.
#[derive(Clone)]
pub struct DiskMonitor {
    /// Path to data directory
    data_dir: PathBuf,
    /// Hard limit in bytes (e.g., 250 GB)
    hard_limit: u64,
    /// Threshold to pause downloads (e.g., 230 GB)
    pause_threshold: u64,
    /// Threshold to resume downloads (e.g., 200 GB)
    resume_threshold: u64,
    /// Cache duration for usage results
    cache_duration: Duration,
    /// Cached usage (protected by mutex for thread safety)
    cached_usage: Arc<Mutex<Option<CachedUsage>>>,
}

impl DiskMonitor {
    /// Create a new disk monitor.
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Path to data directory to monitor
    /// * `hard_limit_gb` - Hard limit in GB (e.g., 250)
    /// * `pause_threshold_gb` - Pause downloads at this limit in GB (e.g., 230)
    /// * `resume_threshold_gb` - Resume downloads below this limit in GB (e.g., 200)
    /// * `cache_duration` - How long to cache usage results (e.g., 5 seconds)
    pub fn new(
        data_dir: impl AsRef<Path>,
        hard_limit_gb: u64,
        pause_threshold_gb: u64,
        resume_threshold_gb: u64,
        cache_duration: Duration,
    ) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();

        // Validate thresholds
        if pause_threshold_gb > hard_limit_gb {
            anyhow::bail!(
                "Pause threshold ({} GB) cannot exceed hard limit ({} GB)",
                pause_threshold_gb,
                hard_limit_gb
            );
        }

        if resume_threshold_gb > pause_threshold_gb {
            anyhow::bail!(
                "Resume threshold ({} GB) cannot exceed pause threshold ({} GB)",
                resume_threshold_gb,
                pause_threshold_gb
            );
        }

        Ok(Self {
            data_dir,
            hard_limit: hard_limit_gb * 1_000_000_000,
            pause_threshold: pause_threshold_gb * 1_000_000_000,
            resume_threshold: resume_threshold_gb * 1_000_000_000,
            cache_duration,
            cached_usage: Arc::new(Mutex::new(None)),
        })
    }

    /// Get current disk usage, using cache if available.
    pub fn current_usage(&self) -> Result<DiskUsage> {
        // Check cache first
        {
            let cached = self.cached_usage.lock().unwrap();
            if let Some(cached) = cached.as_ref() {
                if cached.timestamp.elapsed() < self.cache_duration {
                    debug!("Using cached disk usage");
                    return Ok(cached.usage.clone());
                }
            }
        }

        // Cache miss or expired, recalculate
        debug!("Calculating disk usage");
        let usage = self.calculate_usage()?;

        // Update cache
        {
            let mut cached = self.cached_usage.lock().unwrap();
            *cached = Some(CachedUsage {
                usage: usage.clone(),
                timestamp: Instant::now(),
            });
        }

        info!(
            total_gb = usage.total_gb(),
            videos_gb = usage.videos_bytes as f64 / 1_000_000_000.0,
            transcripts_gb = usage.transcripts_bytes as f64 / 1_000_000_000.0,
            percentage = usage.percentage(self.hard_limit),
            "Disk usage"
        );

        Ok(usage)
    }

    /// Check if downloads should be paused due to disk usage.
    pub fn should_pause_downloads(&self) -> Result<bool> {
        let usage = self.current_usage()?;
        let should_pause = usage.total_bytes >= self.pause_threshold;

        if should_pause {
            warn!(
                current_gb = usage.total_gb(),
                threshold_gb = self.pause_threshold as f64 / 1_000_000_000.0,
                percentage = usage.percentage(self.hard_limit),
                "Disk usage exceeded pause threshold"
            );
        }

        Ok(should_pause)
    }

    /// Check if downloads can resume.
    pub fn can_resume_downloads(&self) -> Result<bool> {
        let usage = self.current_usage()?;
        let can_resume = usage.total_bytes < self.resume_threshold;

        if can_resume {
            info!(
                current_gb = usage.total_gb(),
                threshold_gb = self.resume_threshold as f64 / 1_000_000_000.0,
                percentage = usage.percentage(self.hard_limit),
                "Disk usage below resume threshold"
            );
        }

        Ok(can_resume)
    }

    /// Get detailed space breakdown with analysis.
    pub fn get_breakdown(&self) -> Result<SpaceBreakdown> {
        let usage = self.current_usage()?;
        let percentage = usage.percentage(self.hard_limit);
        let available_bytes = self.hard_limit.saturating_sub(usage.total_bytes);
        let can_download = usage.total_bytes < self.pause_threshold;

        Ok(SpaceBreakdown {
            usage,
            percentage,
            available_bytes,
            can_download,
        })
    }

    /// Invalidate cache to force recalculation on next access.
    pub fn invalidate_cache(&self) {
        let mut cached = self.cached_usage.lock().unwrap();
        *cached = None;
        debug!("Invalidated disk usage cache");
    }

    /// Calculate actual disk usage by walking directories.
    fn calculate_usage(&self) -> Result<DiskUsage> {
        let videos_bytes = self.calculate_dir_size(&self.data_dir.join("videos"))?;
        let audio_bytes = self.calculate_dir_size(&self.data_dir.join("audio"))?;
        let transcripts_bytes = self.calculate_dir_size(&self.data_dir.join("transcripts"))?;
        let tokens_bytes = self.calculate_dir_size(&self.data_dir.join("tokens"))?;
        let cache_bytes = self.calculate_dir_size(&self.data_dir.join("cache"))?;
        let analysis_bytes = self.calculate_dir_size(&self.data_dir.join("analysis"))?;

        // Database file
        let db_path = self.data_dir.join("jobs.db");
        let db_bytes = if db_path.exists() {
            std::fs::metadata(&db_path)
                .context("Failed to get database file size")?
                .len()
        } else {
            0
        };

        // Logs directory
        let logs_bytes = self.calculate_dir_size(&self.data_dir.join("logs"))?;

        let other_bytes = analysis_bytes + logs_bytes;

        let total_bytes = videos_bytes
            + audio_bytes
            + transcripts_bytes
            + tokens_bytes
            + cache_bytes
            + db_bytes
            + other_bytes;

        Ok(DiskUsage {
            total_bytes,
            videos_bytes,
            audio_bytes,
            transcripts_bytes,
            tokens_bytes,
            cache_bytes,
            db_bytes,
            other_bytes,
        })
    }

    /// Calculate total size of a directory recursively.
    fn calculate_dir_size(&self, path: &Path) -> Result<u64> {
        if !path.exists() {
            return Ok(0);
        }

        let mut total = 0u64;

        let entries = std::fs::read_dir(path)
            .with_context(|| format!("Failed to read directory: {}", path.display()))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let metadata = entry
                .metadata()
                .context("Failed to get entry metadata")?;

            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                total += self.calculate_dir_size(&entry.path())?;
            }
        }

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_disk_usage_percentage() {
        let usage = DiskUsage {
            total_bytes: 100_000_000_000, // 100 GB
            videos_bytes: 50_000_000_000,
            audio_bytes: 10_000_000_000,
            transcripts_bytes: 20_000_000_000,
            tokens_bytes: 10_000_000_000,
            cache_bytes: 5_000_000_000,
            db_bytes: 3_000_000_000,
            other_bytes: 2_000_000_000,
        };

        assert_eq!(usage.percentage(250_000_000_000), 40.0); // 100 GB / 250 GB = 40%
        assert_eq!(usage.total_gb(), 100.0);
    }

    #[test]
    fn test_disk_monitor_thresholds() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create monitor with 1 GB limits
        let monitor = DiskMonitor::new(
            temp_dir.path(),
            1, // 1 GB hard limit
            1, // 1 GB pause (same as limit for test)
            0, // 0 GB resume
            Duration::from_secs(1),
        )?;

        // Should not pause when empty
        assert!(!monitor.should_pause_downloads()?);
        assert!(monitor.can_resume_downloads()?);

        Ok(())
    }

    #[test]
    fn test_calculate_dir_size() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let videos_dir = temp_dir.path().join("videos");
        fs::create_dir_all(&videos_dir)?;

        // Create test files
        fs::write(videos_dir.join("test1.mp4"), vec![0u8; 1000])?;
        fs::write(videos_dir.join("test2.mp4"), vec![0u8; 2000])?;

        let monitor = DiskMonitor::new(
            temp_dir.path(),
            10,
            9,
            8,
            Duration::from_secs(1),
        )?;

        let size = monitor.calculate_dir_size(&videos_dir)?;
        assert_eq!(size, 3000);

        Ok(())
    }

    #[test]
    fn test_cache_expiration() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let monitor = DiskMonitor::new(
            temp_dir.path(),
            10,
            9,
            8,
            Duration::from_millis(100), // Very short cache
        )?;

        // First call should calculate
        let usage1 = monitor.current_usage()?;

        // Second immediate call should use cache
        let usage2 = monitor.current_usage()?;
        assert_eq!(usage1.total_bytes, usage2.total_bytes);

        // Wait for cache to expire
        std::thread::sleep(Duration::from_millis(150));

        // Should recalculate
        let usage3 = monitor.current_usage()?;
        assert_eq!(usage3.total_bytes, usage1.total_bytes);

        Ok(())
    }
}
