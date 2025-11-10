//! Anime downloader implementation.
//!
//! Downloads anime episodes using animdl with disk-aware coordination.

use anyhow::{Context, Result};
use shared::{DataPaths, DiskMonitor, Job, JobQueue, JobStage};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Anime downloader worker.
pub struct AnimeDownloader {
    /// Worker ID for logging
    worker_id: usize,
    /// Job queue
    queue: Arc<Mutex<JobQueue>>,
    /// Disk monitor
    disk_monitor: DiskMonitor,
    /// Data paths
    data_paths: DataPaths,
    /// Dry run mode (don't actually download)
    dry_run: bool,
    /// Number of completed downloads
    completed: usize,
    /// Number of failed downloads
    failed: usize,
}

impl AnimeDownloader {
    /// Create a new downloader worker.
    pub fn new(
        worker_id: usize,
        queue: Arc<Mutex<JobQueue>>,
        disk_monitor: DiskMonitor,
        data_paths: DataPaths,
        dry_run: bool,
    ) -> Self {
        Self {
            worker_id,
            queue,
            disk_monitor,
            data_paths,
            dry_run,
            completed: 0,
            failed: 0,
        }
    }

    /// Get worker ID.
    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    /// Run the download worker loop.
    pub async fn run(&mut self) -> Result<()> {
        info!(worker_id = self.worker_id, "Download worker started");

        loop {
            // Check disk space before attempting download
            if self.disk_monitor.should_pause_downloads()? {
                self.wait_for_space().await?;
            }

            // Try to get next job from queue
            let job = match self.queue.lock().unwrap().dequeue_next(JobStage::Queued) {
                Ok(job) => job,
                Err(e) => {
                    // Check if error is "no jobs available"
                    let err_msg = format!("{}", e);
                    if err_msg.contains("No jobs available") {
                        debug!(worker_id = self.worker_id, "No more jobs in queue");
                        break;
                    }
                    return Err(e).context("Failed to dequeue job");
                }
            };

            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                anime_title = %job.anime_title,
                episode = job.episode,
                "Processing job"
            );

            // Update job stage to downloading
            self.queue
                .lock()
                .unwrap()
                .update_stage(job.id, JobStage::Downloading)
                .context("Failed to update job stage")?;

            // Download the episode
            match self.download_episode(&job).await {
                Ok(video_path) => {
                    // Get file size
                    let video_size = std::fs::metadata(&video_path)
                        .context("Failed to get video file size")?
                        .len();

                    info!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        video_size_mb = video_size / 1_000_000,
                        "Download complete"
                    );

                    // Update job with file path and size
                    self.queue
                        .lock()
                        .unwrap()
                        .update_job_with_video(job.id, video_path, video_size)
                        .context("Failed to update job with video info")?;

                    // Update stage to downloaded
                    self.queue
                        .lock()
                        .unwrap()
                        .update_stage(job.id, JobStage::Downloaded)
                        .context("Failed to update job stage")?;

                    self.completed += 1;

                    // Invalidate disk cache to reflect new file
                    self.disk_monitor.invalidate_cache();
                }
                Err(e) => {
                    error!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        error = %e,
                        "Download failed"
                    );

                    // Check if we should retry
                    if job.retry_count < job.max_retries {
                        warn!(
                            job_id = job.id,
                            retry_count = job.retry_count + 1,
                            max_retries = job.max_retries,
                            "Retrying job"
                        );

                        // Increment retry count and reset to queued
                        self.queue
                            .lock()
                            .unwrap()
                            .increment_retry(job.id)
                            .context("Failed to increment retry count")?;
                        self.queue
                            .lock()
                            .unwrap()
                            .update_stage(job.id, JobStage::Queued)
                            .context("Failed to reset job stage")?;
                    } else {
                        error!(
                            job_id = job.id,
                            "Max retries exceeded, marking job as failed"
                        );

                        // Mark as failed
                        self.queue
                            .lock()
                            .unwrap()
                            .update_stage_with_error(job.id, JobStage::Failed, format!("{:#}", e))
                            .context("Failed to update job as failed")?;

                        self.failed += 1;
                    }
                }
            }
        }

        info!(
            worker_id = self.worker_id,
            completed = self.completed,
            failed = self.failed,
            "Download worker finished"
        );

        Ok(())
    }

    /// Wait for disk space to be freed.
    async fn wait_for_space(&self) -> Result<()> {
        info!(
            worker_id = self.worker_id,
            "Disk space limit reached, pausing downloads"
        );

        loop {
            // Wait before checking again
            sleep(Duration::from_secs(30)).await;

            if self.disk_monitor.can_resume_downloads()? {
                info!(
                    worker_id = self.worker_id,
                    "Disk space freed, resuming downloads"
                );
                break;
            }

            let usage = self.disk_monitor.current_usage()?;
            debug!(
                worker_id = self.worker_id,
                current_gb = usage.total_gb(),
                "Waiting for space to be freed"
            );
        }

        Ok(())
    }

    /// Download a single episode using animdl.
    async fn download_episode(&self, job: &Job) -> Result<PathBuf> {
        // Determine output directory
        let output_dir = self.data_paths.video_dir(job.mal_id);

        // Build output filename
        let safe_title = sanitize_filename(&job.anime_title);
        let filename = format!("{}_ep{:03}.mp4", safe_title, job.episode);
        let output_path = output_dir.join(&filename);

        // Check if file already exists
        if output_path.exists() {
            warn!(
                job_id = job.id,
                path = %output_path.display(),
                "Video file already exists, skipping download"
            );
            return Ok(output_path);
        }

        if self.dry_run {
            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                "Dry run mode: would download {} episode {}",
                job.anime_title,
                job.episode
            );

            // Create empty file for testing
            std::fs::write(&output_path, b"")?;
            return Ok(output_path);
        }

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            anime_title = %job.anime_title,
            episode = job.episode,
            output_path = %output_path.display(),
            "Starting download with animdl"
        );

        // Build animdl command
        // animdl download "anime title" --range episode_num --auto-select --quality best
        let status = Command::new("animdl")
            .arg("download")
            .arg(&job.anime_title)
            .arg("--range")
            .arg(format!("{}", job.episode))
            .arg("--auto-select")
            .arg("--quality")
            .arg("best")
            .arg("--output")
            .arg(&output_dir)
            .status()
            .context("Failed to execute animdl command")?;

        if !status.success() {
            anyhow::bail!(
                "animdl failed with exit code: {:?}",
                status.code().unwrap_or(-1)
            );
        }

        // Verify file was created
        if !output_path.exists() {
            anyhow::bail!("Video file was not created: {}", output_path.display());
        }

        Ok(output_path)
    }
}

/// Sanitize filename by removing/replacing invalid characters.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            sanitize_filename("Fullmetal Alchemist: Brotherhood"),
            "Fullmetal Alchemist_ Brotherhood"
        );
        assert_eq!(
            sanitize_filename("Attack on Titan: Season 2"),
            "Attack on Titan_ Season 2"
        );
        assert_eq!(sanitize_filename("Normal Title"), "Normal Title");
        assert_eq!(
            sanitize_filename("Title/with\\invalid:chars"),
            "Title_with_invalid_chars"
        );
    }
}
