//! Transcriber with aggressive cleanup for disk space management.
//!
//! This binary transcribes audio from downloaded videos using Whisper,
//! and immediately deletes video and audio files to free up disk space.

use anyhow::{Context, Result};
use clap::Parser;
use shared::{Config, Database, DataPaths, DiskMonitor, JobQueue};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info};

mod transcriber;

use transcriber::Transcriber;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Number of concurrent transcription workers
    #[arg(short = 'w', long)]
    workers: Option<usize>,

    /// Whisper model to use (tiny, base, small, medium, large)
    #[arg(short = 'm', long, default_value = "base")]
    model: String,

    /// Dry run (don't actually transcribe, for testing)
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
        component: "transcriber".to_string(),
        default_level: log_level,
        console: true,
        file: true,
        json_format: false,
    })?;

    info!("Transcriber starting");
    info!(config_file = %args.config.display(), "Loaded configuration");
    info!(
        workers = args.workers.unwrap_or(config.disk_management.max_concurrent_transcriptions),
        model = %args.model,
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
        videos_gb = breakdown.usage.videos_bytes as f64 / 1_000_000_000.0,
        percentage = breakdown.percentage,
        "Initial disk usage"
    );

    // Get number of workers
    let num_workers = args
        .workers
        .unwrap_or(config.disk_management.max_concurrent_transcriptions);

    // Check queue status
    let queue_stats = job_queue
        .get_queue_stats()
        .context("Failed to get queue stats")?;
    info!(
        downloaded = queue_stats.downloaded,
        transcribing = queue_stats.transcribing,
        transcribed = queue_stats.transcribed,
        "Initial queue status"
    );

    if queue_stats.downloaded == 0 && queue_stats.transcribing == 0 {
        info!("No jobs to process, exiting");
        return Ok(());
    }

    // Wrap queue in Arc for sharing between workers
    let job_queue = Arc::new(Mutex::new(job_queue));

    // Initialize transcribers
    let mut transcribers = Vec::new();
    for worker_id in 0..num_workers {
        let transcriber = Transcriber::new(
            worker_id,
            Arc::clone(&job_queue),
            disk_monitor.clone(),
            data_paths.clone(),
            args.model.clone(),
            config.disk_management.cleanup.clone(),
            args.dry_run,
        );
        transcribers.push(transcriber);
    }

    info!(num_workers, "Starting transcription workers");

    // Spawn worker tasks
    let mut handles = Vec::new();
    for mut transcriber in transcribers {
        let handle = tokio::spawn(async move {
            if let Err(e) = transcriber.run().await {
                error!(worker_id = transcriber.worker_id(), error = %e, "Worker failed");
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
    info!("=== Transcription Complete ===");
    info!("Downloaded: {}", final_stats.downloaded);
    info!("Transcribing: {}", final_stats.transcribing);
    info!("Transcribed: {}", final_stats.transcribed);
    info!("Failed: {}", final_stats.failed);

    let final_breakdown = disk_monitor.get_breakdown()?;
    info!(
        total_gb = final_breakdown.usage.total_gb(),
        videos_gb = final_breakdown.usage.videos_bytes as f64 / 1_000_000_000.0,
        transcripts_gb = final_breakdown.usage.transcripts_bytes as f64 / 1_000_000_000.0,
        percentage = final_breakdown.percentage,
        "Final disk usage"
    );

    info!("Transcriber finished successfully");

    Ok(())
}
