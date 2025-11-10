//! Anime downloader with disk-aware coordination.
//!
//! This binary downloads anime episodes from the job queue, with automatic
//! pausing when disk usage approaches limits.

use anyhow::{Context, Result};
use clap::Parser;
use shared::{Config, Database, DataPaths, DiskMonitor, JobQueue};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info, warn};

mod downloader;

use downloader::AnimeDownloader;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Number of concurrent download workers
    #[arg(short = 'w', long)]
    workers: Option<usize>,

    /// Dry run (don't actually download)
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config = Config::from_file(&args.config)
        .with_context(|| format!("Failed to load config from {}", args.config.display()))?;

    // Initialize logging
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    shared::logging::init(shared::LogConfig {
        log_dir: config.log_dir().to_string_lossy().to_string(),
        component: "anime-downloader".to_string(),
        default_level: log_level,
        console: true,
        file: true,
        json_format: false,
    })?;

    info!("Anime Downloader starting");
    info!(config_file = %args.config.display(), "Loaded configuration");
    info!(
        workers = args.workers.unwrap_or(config.disk_management.max_concurrent_downloads),
        dry_run = args.dry_run,
        "Runtime configuration"
    );

    // Initialize data paths
    let data_paths = DataPaths::new(config.data_dir());
    data_paths
        .create_dirs()
        .context("Failed to create data directories")?;

    // Initialize database
    let db_path = config.database_path();
    info!(db_path = %db_path.display(), "Opening database");
    let database = Database::open(&db_path).context("Failed to open database")?;
    let job_queue = JobQueue::new(database);

    // Initialize disk monitor
    let disk_monitor = DiskMonitor::new(
        config.data_dir(),
        config.disk_management.hard_limit_gb,
        config.disk_management.pause_threshold_gb,
        config.disk_management.resume_threshold_gb,
        Duration::from_secs(config.disk_management.cache_duration_seconds),
    )
    .context("Failed to initialize disk monitor")?;

    // Check initial disk usage
    let breakdown = disk_monitor.get_breakdown()?;
    info!(
        total_gb = breakdown.usage.total_gb(),
        percentage = breakdown.percentage,
        can_download = breakdown.can_download,
        "Initial disk usage"
    );

    if !breakdown.can_download {
        warn!(
            "Disk usage already exceeds pause threshold ({:.1} GB / {:.1} GB)",
            breakdown.usage.total_gb(),
            config.disk_management.pause_threshold_gb as f64
        );
        warn!("Waiting for transcriber to free up space...");
    }

    // Get number of workers
    let num_workers = args
        .workers
        .unwrap_or(config.disk_management.max_concurrent_downloads);

    // Check queue status
    let queue_stats = job_queue
        .get_queue_stats()
        .context("Failed to get queue stats")?;
    info!(
        queued = queue_stats.queued,
        downloading = queue_stats.downloading,
        downloaded = queue_stats.downloaded,
        "Initial queue status"
    );

    if queue_stats.queued == 0 && queue_stats.downloading == 0 {
        info!("No jobs to process, exiting");
        return Ok(());
    }

    // Wrap queue in Arc for sharing between workers
    let job_queue = Arc::new(Mutex::new(job_queue));

    // Initialize downloaders
    let mut downloaders = Vec::new();
    for worker_id in 0..num_workers {
        let downloader = AnimeDownloader::new(
            worker_id,
            Arc::clone(&job_queue),
            disk_monitor.clone(),
            data_paths.clone(),
            args.dry_run,
        );
        downloaders.push(downloader);
    }

    info!(num_workers, "Starting download workers");

    // Spawn worker tasks
    let mut handles = Vec::new();
    for mut downloader in downloaders {
        let handle = tokio::spawn(async move {
            if let Err(e) = downloader.run().await {
                error!(worker_id = downloader.worker_id(), error = %e, "Worker failed");
                return Err(e);
            }
            Ok(())
        });
        handles.push(handle);
    }

    // Wait for all workers to complete
    info!("Waiting for workers to complete");
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(())) => {
                info!(worker_id = i, "Worker completed successfully");
            }
            Ok(Err(e)) => {
                error!(worker_id = i, error = %e, "Worker failed");
            }
            Err(e) => {
                error!(worker_id = i, error = %e, "Worker panicked");
            }
        }
    }

    // Final statistics
    let final_stats = job_queue
        .lock()
        .unwrap()
        .get_queue_stats()
        .context("Failed to get final queue stats")?;
    info!("=== Download Complete ===");
    info!("Queued: {}", final_stats.queued);
    info!("Downloading: {}", final_stats.downloading);
    info!("Downloaded: {}", final_stats.downloaded);
    info!("Failed: {}", final_stats.failed);

    let final_breakdown = disk_monitor.get_breakdown()?;
    info!(
        total_gb = final_breakdown.usage.total_gb(),
        videos_gb = final_breakdown.usage.videos_bytes as f64 / 1_000_000_000.0,
        percentage = final_breakdown.percentage,
        "Final disk usage"
    );

    info!("Anime Downloader finished successfully");

    Ok(())
}
