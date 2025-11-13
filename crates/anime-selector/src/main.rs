//! Anime Selector - Pre-select correct anime titles using Claude Haiku
//!
//! This tool queries AllAnime API for each anime in the database and uses
//! Claude Haiku to intelligently select the main series vs specials/OVAs.
//! Results are cached in the anime_selection_cache table.

use anyhow::{Context, Result};
use clap::Parser;
use shared::config::Config;
use shared::db::Database;
use shared::queue::JobQueue;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// Anime Selector CLI arguments
#[derive(Parser, Debug)]
#[command(name = "anime-selector")]
#[command(about = "Pre-select correct anime titles using Claude Haiku")]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Number of concurrent workers
    #[arg(short, long, default_value = "5")]
    workers: usize,

    /// Dry run mode (don't cache selections)
    #[arg(long)]
    dry_run: bool,

    /// Process only specific MAL ID
    #[arg(long)]
    mal_id: Option<u32>,

    /// Review mode: show low-confidence selections only
    #[arg(long)]
    review: bool,
}

#[derive(Debug, serde::Deserialize)]
struct AnimeRecord {
    mal_id: u32,
    title: String,
    title_english: Option<String>,
    episodes_total: Option<i32>,
    year: Option<i32>,
    #[serde(rename = "type")]
    anime_type: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SelectionResult {
    index: i32,
    confidence: String,
    reason: String,
    mal_episodes: Option<i32>,
    selected_episodes: Option<i32>,
    episode_match: Option<String>,
}

#[derive(Debug)]
struct SelectionStats {
    total: usize,
    cached: usize,
    selected: usize,
    high_confidence: usize,
    medium_confidence: usize,
    low_confidence: usize,
    no_candidates: usize,
    errors: usize,
}

impl SelectionStats {
    fn new() -> Self {
        Self {
            total: 0,
            cached: 0,
            selected: 0,
            high_confidence: 0,
            medium_confidence: 0,
            low_confidence: 0,
            no_candidates: 0,
            errors: 0,
        }
    }

    fn print_summary(&self) {
        info!("=== Selection Summary ===");
        info!("Total anime: {}", self.total);
        info!("Already cached: {}", self.cached);
        info!("Newly selected: {}", self.selected);
        info!("  - High confidence: {}", self.high_confidence);
        info!("  - Medium confidence: {}", self.medium_confidence);
        info!("  - Low confidence: {}", self.low_confidence);
        info!("No candidates found: {} (marked as skipped)", self.no_candidates);
        info!("Errors: {}", self.errors);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    shared::logging::init_for_component("anime-selector", "data/logs")?;

    info!("Starting anime selector");
    info!("Workers: {}", args.workers);
    if args.dry_run {
        info!("DRY RUN MODE - selections will not be cached");
    }

    // Load configuration
    let config = Config::from_file(&args.config)
        .with_context(|| format!("Failed to load config from {:?}", args.config))?;

    // Open database (use database_path() to get correct absolute path)
    let db_path = config.database_path();
    let db = Database::open(&db_path)
        .context("Failed to open database")?;

    // Review mode: just show low-confidence selections
    if args.review {
        return review_selections(&db);
    }

    // Get list of anime to process
    let anime_list = get_anime_list(&db, args.mal_id)?;
    info!("Found {} anime to process", anime_list.len());

    if anime_list.is_empty() {
        info!("No anime to process. Run mal-scraper first.");
        return Ok(());
    }

    // Process anime with concurrent workers
    let stats = process_anime_batch(
        anime_list,
        &config,
        args.workers,
        args.dry_run,
    ).await?;

    // Print summary
    stats.print_summary();

    Ok(())
}

/// Get list of anime from database
fn get_anime_list(db: &Database, mal_id: Option<u32>) -> Result<Vec<AnimeRecord>> {
    let conn = db.conn();

    let query = if let Some(id) = mal_id {
        format!(
            "SELECT mal_id, title, title_english, episodes_total, year, type
             FROM anime WHERE mal_id = {}",
            id
        )
    } else {
        "SELECT mal_id, title, title_english, episodes_total, year, type
         FROM anime
         ORDER BY rank ASC".to_string()
    };

    let mut stmt = conn.prepare(&query)?;
    let anime_iter = stmt.query_map([], |row| {
        Ok(AnimeRecord {
            mal_id: row.get(0)?,
            title: row.get(1)?,
            title_english: row.get(2)?,
            episodes_total: row.get(3)?,
            year: row.get(4)?,
            anime_type: row.get(5)?,
        })
    })?;

    let mut anime_list = Vec::new();
    for anime in anime_iter {
        anime_list.push(anime?);
    }

    Ok(anime_list)
}

/// Process batch of anime with concurrent workers
async fn process_anime_batch(
    anime_list: Vec<AnimeRecord>,
    config: &Config,
    workers: usize,
    dry_run: bool,
) -> Result<SelectionStats> {
    let stats = Arc::new(tokio::sync::Mutex::new(SelectionStats::new()));
    let semaphore = Arc::new(Semaphore::new(workers));
    let db_path = config.database_path().to_string_lossy().to_string();
    let api_key = config.anthropic.api_key.clone();

    let mut tasks = Vec::new();

    for anime in anime_list {
        let sem_permit = semaphore.clone().acquire_owned().await?;
        let stats_clone = stats.clone();
        let db_path_clone = db_path.clone();
        let api_key_clone = api_key.clone();

        let task = tokio::spawn(async move {
            let result = process_anime(anime, &db_path_clone, &api_key_clone, dry_run).await;

            // Update stats
            let mut stats_guard = stats_clone.lock().await;
            stats_guard.total += 1;

            match &result {
                Ok(Some(ref confidence)) => {
                    match confidence.as_str() {
                        "no_candidates" => {
                            stats_guard.no_candidates += 1;
                        }
                        "high" | "medium" | "low" => {
                            stats_guard.selected += 1;
                            match confidence.as_str() {
                                "high" => stats_guard.high_confidence += 1,
                                "medium" => stats_guard.medium_confidence += 1,
                                "low" => stats_guard.low_confidence += 1,
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None) => {
                    stats_guard.cached += 1;
                }
                Err(_) => {
                    stats_guard.errors += 1;
                }
            }

            drop(sem_permit);
            result
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    let final_stats = stats.lock().await.clone();
    Ok(final_stats)
}

/// Process a single anime
async fn process_anime(
    anime: AnimeRecord,
    db_path: &str,
    api_key: &str,
    dry_run: bool,
) -> Result<Option<String>> {
    // Check if already cached
    let db = Database::open(db_path)?;
    let mut queue = JobQueue::new(db);

    if let Some(_selection) = queue.get_selection(anime.mal_id)? {
        debug!(
            mal_id = anime.mal_id,
            title = %anime.title,
            "Using cached selection"
        );
        return Ok(None);
    }

    info!(
        mal_id = anime.mal_id,
        title = %anime.title,
        "Selecting anime"
    );

    // Get candidates from AllAnime
    let candidates = match get_anime_candidates(&anime.title).await {
        Ok(c) if !c.is_empty() => c,
        Ok(_) | Err(_) => {
            // No candidates found or API error - mark as skipped
            warn!(
                mal_id = anime.mal_id,
                title = %anime.title,
                "No candidates found from AllAnime, marking as skipped"
            );

            // Cache with special marker (selected_index = -1)
            if !dry_run {
                queue.cache_selection(
                    anime.mal_id,
                    &anime.title,
                    &anime.title,
                    -1,  // Special marker for "no candidates"
                    "N/A",
                    "no_candidates",
                    Some("No candidates found from AllAnime (likely adult content or not available)"),
                    anime.episodes_total,
                    None,
                    Some("unknown"),
                )?;
            }

            return Ok(Some("no_candidates".to_string()));
        }
    };

    debug!(
        mal_id = anime.mal_id,
        candidates = ?candidates,
        "Got candidates from AllAnime"
    );

    // Use Claude to select
    let selection_result = match select_with_claude(&anime, &candidates, api_key).await {
        Ok(r) => r,
        Err(e) => {
            error!(
                mal_id = anime.mal_id,
                title = %anime.title,
                error = %e,
                "Failed to select with Claude"
            );
            return Err(e);
        }
    };

    let selected_title = candidates.get((selection_result.index - 1) as usize)
        .cloned()
        .unwrap_or_else(|| candidates[0].clone());

    info!(
        mal_id = anime.mal_id,
        title = %anime.title,
        selected = %selected_title,
        confidence = %selection_result.confidence,
        reason = %selection_result.reason,
        "Selection complete"
    );

    // Cache the selection (unless dry run)
    if !dry_run {
        queue.cache_selection(
            anime.mal_id,
            &anime.title,
            &anime.title,
            selection_result.index,
            &selected_title,
            &selection_result.confidence,
            Some(&selection_result.reason),
            selection_result.mal_episodes,
            selection_result.selected_episodes,
            selection_result.episode_match.as_deref(),
        )?;
    }

    Ok(Some(selection_result.confidence))
}

/// Get anime candidates from AllAnime API
async fn get_anime_candidates(title: &str) -> Result<Vec<String>> {
    let output = Command::new("zsh")
        .arg("scripts/get_anime_candidates.sh")
        .arg(title)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute get_anime_candidates.sh")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("get_anime_candidates.sh failed: {}", stderr));
    }

    let candidates: Vec<String> = serde_json::from_slice(&output.stdout)
        .context("Failed to parse candidates JSON")?;

    Ok(candidates)
}

/// Select anime using Claude Haiku
async fn select_with_claude(
    anime: &AnimeRecord,
    candidates: &[String],
    api_key: &str,
) -> Result<SelectionResult> {
    let candidates_json = serde_json::to_string(candidates)?;

    // Helper function to quote arguments for use inside zsh -c '...'
    // Use double quotes and escape ", $, `, and \
    fn shell_quote(s: &str) -> String {
        let escaped = s
            .replace('\\', r"\\")
            .replace('"', r#"\""#)
            .replace('$', r"\$")
            .replace('`', r"\`");
        format!(r#""{}""#, escaped)
    }

    // Helper function to use single quotes (for strings without variables/substitution)
    // Single quotes prevent all special character processing including !
    // To include a literal single quote, we end the quote, add \', and start a new quote
    fn shell_quote_single(s: &str) -> String {
        let escaped = s.replace('\'', r"'\''");
        format!("'{}'", escaped)
    }

    // Build Python command with properly quoted arguments
    // Use single quotes for candidates JSON to avoid ! expansion issues
    let mut python_cmd = format!(
        "scripts/select_anime.py --mal-title {} --candidates {}",
        shell_quote(&anime.title),
        shell_quote_single(&candidates_json)
    );

    if let Some(episodes) = anime.episodes_total {
        python_cmd.push_str(&format!(" --episodes {}", episodes));
    }

    if let Some(year) = anime.year {
        python_cmd.push_str(&format!(" --year {}", year));
    }

    if let Some(ref anime_type) = anime.anime_type {
        python_cmd.push_str(&format!(" --anime-type {}", shell_quote(anime_type)));
    }

    if !api_key.is_empty() {
        python_cmd.push_str(&format!(" --api-key {}", shell_quote(api_key)));
    }

    // Use zsh with conda activation - CRITICAL: zsh required for conda
    // Disable history expansion with 'set +H' to prevent ! from being escaped
    let full_cmd = format!(
        r#"set +H && eval "$(conda shell.zsh hook)" && conda activate GDA2025 && python3 {}"#,
        python_cmd
    );

    debug!("Executing command: zsh -c '{}'", full_cmd);

    let mut cmd = Command::new("zsh");
    cmd.arg("-c").arg(&full_cmd);

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute select_anime.py")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        error!(
            "select_anime.py failed\nstdout: {}\nstderr: {}",
            stdout, stderr
        );
        return Err(anyhow::anyhow!(
            "select_anime.py failed with exit code {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            stdout,
            stderr
        ));
    }

    let result: SelectionResult = serde_json::from_slice(&output.stdout)
        .context("Failed to parse selection result JSON")?;

    Ok(result)
}

/// Review low-confidence selections
fn review_selections(db: &Database) -> Result<()> {
    info!("=== Low Confidence Selections ===");

    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT mal_id, anime_title, selected_title, confidence, reason
         FROM anime_selection_cache
         WHERE confidence = 'low'
         ORDER BY mal_id"
    )?;

    let selections = stmt.query_map([], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut count = 0;
    for selection in selections {
        let (mal_id, anime_title, selected_title, confidence, reason) = selection?;
        count += 1;
        println!();
        println!("MAL ID: {}", mal_id);
        println!("Anime: {}", anime_title);
        println!("Selected: {}", selected_title);
        println!("Confidence: {}", confidence);
        if let Some(r) = reason {
            println!("Reason: {}", r);
        }
    }

    println!();
    info!("Total low-confidence selections: {}", count);

    if count > 0 {
        info!("To manually correct a selection, use:");
        info!("  sqlite3 data/jobs.db \"UPDATE anime_selection_cache SET selected_index=N, selected_title='Title' WHERE mal_id=XXXXX\"");
    }

    Ok(())
}

// Implement Clone for SelectionStats
impl Clone for SelectionStats {
    fn clone(&self) -> Self {
        Self {
            total: self.total,
            cached: self.cached,
            selected: self.selected,
            high_confidence: self.high_confidence,
            medium_confidence: self.medium_confidence,
            low_confidence: self.low_confidence,
            no_candidates: self.no_candidates,
            errors: self.errors,
        }
    }
}
