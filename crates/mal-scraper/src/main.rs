//! MAL Scraper CLI application.

use anyhow::{Context, Result};
use clap::Parser;
use mal_scraper::{CacheManager, DiscoveryManager, JikanClient, MalScraper};
use shared::{Config, Database, DataPaths, JobQueue};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Clear cache before running
    #[arg(long)]
    clear_cache: bool,
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
        component: "mal-scraper".to_string(),
        default_level: log_level,
        console: true,
        file: true,
        json_format: false,
    })?;

    info!("MAL Scraper starting");
    info!(config_file = %args.config.display(), "Loaded configuration");

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

    // Initialize cache
    let cache_dir = config.cache_dir();
    let cache = CacheManager::new(&cache_dir, config.mal_scraper.cache.enabled)
        .context("Failed to initialize cache")?;

    if args.clear_cache {
        info!("Clearing cache");
        cache.clear().context("Failed to clear cache")?;
    }

    // Display cache statistics
    let cache_stats = cache.stats().context("Failed to get cache stats")?;
    info!(
        cached_files = cache_stats.total_files,
        cache_size_mb = cache_stats.total_size_bytes / 1_000_000,
        "Cache statistics"
    );

    // Initialize API client
    let client = JikanClient::new(
        config.mal_scraper.base_url.clone(),
        config.mal_scraper.rate_limit.requests_per_second,
        config.mal_scraper.rate_limit.requests_per_minute,
        config.mal_scraper.max_retries,
        config.mal_scraper.retry_delay_ms,
    )
    .context("Failed to create Jikan client")?;

    // Initialize discovery manager
    let discovery = DiscoveryManager::new(
        client,
        cache,
        config.mal_scraper.min_category_items,
    );

    // Initialize scraper
    let mut scraper = MalScraper::new(discovery, job_queue);

    // Run scraper
    info!("Starting MAL scraper process");
    let stats = scraper.run().await.context("Scraper failed")?;

    // Display final statistics
    info!("=== Scraping Complete ===");
    info!("Categories discovered: {}", stats.total_categories);
    info!("Total anime discovered: {}", stats.total_anime_discovered);
    info!("Unique anime: {}", stats.unique_anime);
    info!("Anime saved to database: {}", stats.anime_saved);
    info!("Jobs created: {}", stats.jobs_created);
    info!("Errors: {}", stats.errors);

    // Display job queue statistics
    let queue_stats = scraper.get_queue_stats().context("Failed to get queue stats")?;
    info!("=== Job Queue Statistics ===");
    info!("Queued: {}", queue_stats.queued);
    info!("Downloading: {}", queue_stats.downloading);
    info!("Downloaded: {}", queue_stats.downloaded);
    info!("Transcribing: {}", queue_stats.transcribing);
    info!("Transcribed: {}", queue_stats.transcribed);
    info!("Tokenizing: {}", queue_stats.tokenizing);
    info!("Tokenized: {}", queue_stats.tokenized);
    info!("Analyzing: {}", queue_stats.analyzing);
    info!("Complete: {}", queue_stats.complete);
    info!("Failed: {}", queue_stats.failed);

    info!("MAL Scraper finished successfully");

    Ok(())
}
