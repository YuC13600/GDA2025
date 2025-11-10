# MAL Scraper - Final Technical Specification

This document contains all finalized decisions for the MAL Scraper implementation.

---

## 1. Category Selection Strategy

### Auto-Discovery Mode

**Rule**: Process ALL categories (Genres, Themes, Studios, Demographics) that have **≥50 anime**.

### Categories to Process

```toml
[mal_scraper.categories]
process_genres = true
process_explicit_genres = true   # Includes: Boys Love, Girls Love, Erotica, Hentai
process_themes = true
process_demographics = true      # Shounen, Seinen, Josei, Shoujo, Kids
process_studios = true

min_category_size = 50           # Skip categories with <50 anime

# Manual exclusions (if needed)
exclude_categories = []          # Empty = process all valid categories
```

### Expected Scale

Based on current MAL statistics (2024):

| Category Type | Total Available | Expected to Process (≥50 items) |
|---------------|----------------|----------------------------------|
| Genres | ~42 | ~35-40 |
| Explicit Genres | ~4 | ~2-3 |
| Themes | ~76 | ~40-50 |
| Demographics | ~5 | ~4-5 |
| Studios | ~1000+ | ~50-100 |
| **TOTAL** | | **~130-200 categories** |

**Expected Results**:
- Raw entries: 130-200 categories × 50 anime = **6,500-10,000 entries**
- Unique anime (after deduplication): **~800-1200 anime**
- Total episodes: ~800-1200 × 15 episodes avg = **~12,000-18,000 episodes**

---

## 2. Ranking Strategy

### Only Record Global Ranking

**Decision**: Do NOT create `anime_category_rankings` table. Only record global ranking in `anime` table.

**Rationale**:
- Simplifies implementation
- Global `rank` is sufficient for planned analysis (ranking intervals vs Zipf fit quality)

### Planned Analysis Use Cases

**Ranking Interval Analysis**:
- Compare Zipf's law fit quality across ranking intervals:
  - Top tier (rank 1-100)
  - Mid tier (rank 101-500)
  - Lower tier (rank 501-1000)
  - Unranked (rank 1000+)
- Hypothesis: Higher-ranked anime may show different linguistic patterns

**Implementation in Analyzer**:
```rust
// Example analysis grouping
let top_tier = anime.iter().filter(|a| a.rank <= 100);
let mid_tier = anime.iter().filter(|a| a.rank > 100 && a.rank <= 500);
// ... analyze Zipf parameters for each group
```

---

## 3. API Configuration

### Rate Limiting (Conservative Strategy)

```toml
[mal_scraper.api]
# Conservative rate limiting (safe margins)
requests_per_second = 2          # Jikan limit: 3/s
requests_per_minute = 50         # Jikan limit: 60/min
max_concurrent_requests = 1      # No parallel requests

# Timeouts
request_timeout_seconds = 30
connect_timeout_seconds = 10

# User agent
user_agent = "GDA2025-Zipf-Analysis/0.1 (Research Project)"
```

### Retry Strategy

```toml
[mal_scraper.retry]
max_retries = 5
initial_delay_ms = 1000          # 1 second
max_delay_ms = 60000             # 1 minute
backoff_factor = 2.0             # Exponential: 1s, 2s, 4s, 8s, 16s

# Retryable errors
retry_on = [
    "timeout",
    "network_error",
    "429_too_many_requests",
    "500_internal_server_error",
    "502_bad_gateway",
    "503_service_unavailable",
    "504_gateway_timeout"
]

# Non-retryable errors
no_retry_on = [
    "404_not_found",
    "403_forbidden",
    "401_unauthorized",
    "400_bad_request"
]
```

**Exponential Backoff Schedule**:
- Attempt 1: Immediate
- Attempt 2: 1 second delay
- Attempt 3: 2 seconds delay
- Attempt 4: 4 seconds delay
- Attempt 5: 8 seconds delay
- Attempt 6: 16 seconds delay (capped at 60s)

---

## 4. Caching Strategy

### Permanent Cache

```toml
[mal_scraper.cache]
enabled = true
cache_dir = "data/cache/mal_cache"
never_expire = true              # Cache永久有效

# Cache validation
validate_on_startup = false      # Don't validate on startup (faster)
auto_refresh_on_missing = true   # Re-fetch if cache file missing

# Cache structure
cache_format = "json"            # JSON format for human readability
compress_cache = false           # No compression (prioritize speed)
```

### Cache Directory Structure

```
data/cache/mal_cache/
├── metadata.json                       # Cache metadata and stats
├── categories/
│   ├── genres/
│   │   ├── action_top50.json          # ~50 KB
│   │   ├── adventure_top50.json       # ~50 KB
│   │   └── ...
│   ├── themes/
│   │   ├── school_top50.json          # ~50 KB
│   │   ├── military_top50.json        # ~50 KB
│   │   └── ...
│   ├── demographics/
│   │   ├── shounen_top50.json         # ~50 KB
│   │   └── ...
│   └── studios/
│       ├── bones_top50.json           # ~50 KB
│       └── ...
└── anime/
    ├── 5114_fullmetal.json            # ~10 KB (Fullmetal Alchemist: Brotherhood)
    ├── 1535_deathnote.json            # ~10 KB
    └── ...
```

**Cache Size Estimation**:
- Category lists: 200 categories × 50 KB = **10 MB**
- Anime metadata: 1000 anime × 10 KB = **10 MB**
- **Total cache**: ~20 MB (permanent)

---

## 5. Database Schema

### Anime Table (Updated)

```sql
CREATE TABLE anime (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mal_id INTEGER UNIQUE NOT NULL,

    -- Titles
    title TEXT NOT NULL,
    title_english TEXT,
    title_japanese TEXT,
    title_synonyms TEXT,              -- JSON array of alternative titles

    -- Type and status
    type TEXT,                         -- TV, Movie, OVA, Special, ONA, Music
    episodes_total INTEGER,
    status TEXT,                       -- Finished Airing, Currently Airing, Not yet aired

    -- Dates
    aired_from DATE,
    aired_to DATE,
    season TEXT,                       -- spring, summer, fall, winter
    year INTEGER,

    -- Classification (JSON arrays)
    genres TEXT,                       -- ["Action", "Adventure", ...]
    explicit_genres TEXT,              -- ["Boys Love", ...]
    themes TEXT,                       -- ["School", "Military", ...]
    demographics TEXT,                 -- ["Shounen", ...]
    studios TEXT,                      -- ["Bones", ...]

    -- Scores and rankings
    score REAL,                        -- Average score (0-10)
    scored_by INTEGER,                 -- Number of users who scored
    rank INTEGER,                      -- ⭐ Global ranking (for interval analysis)
    popularity INTEGER,                -- Popularity ranking (by favorites)

    -- Additional metadata
    source TEXT,                       -- manga, light_novel, original, etc.
    rating TEXT,                       -- G, PG, PG-13, R, R+, Rx
    duration_minutes INTEGER,          -- Average episode duration

    -- Processing status
    episodes_processed INTEGER DEFAULT 0,
    processing_status TEXT DEFAULT 'pending',  -- pending, processing, completed, failed

    -- Timestamps
    fetched_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_mal_id ON anime(mal_id);
CREATE INDEX idx_rank ON anime(rank);                    -- For ranking interval analysis
CREATE INDEX idx_score ON anime(score);
CREATE INDEX idx_processing_status ON anime(processing_status);
```

**Key Decision**: Only `rank` field (global ranking) is stored. No per-category rankings.

---

## 6. Metadata Format

### Anime Metadata JSON

```json
{
  "mal_id": 5114,

  "title": "Fullmetal Alchemist: Brotherhood",
  "title_english": "Fullmetal Alchemist: Brotherhood",
  "title_japanese": "鋼の錬金術師 FULLMETAL ALCHEMIST",
  "title_synonyms": ["Hagane no Renkinjutsushi", "FMA", "FMAB"],

  "type": "TV",
  "episodes": 64,
  "status": "Finished Airing",

  "aired": {
    "from": "2009-04-05T00:00:00+00:00",
    "to": "2010-07-04T00:00:00+00:00"
  },
  "season": "spring",
  "year": 2009,

  "genres": [
    {"mal_id": 1, "name": "Action"},
    {"mal_id": 2, "name": "Adventure"},
    {"mal_id": 8, "name": "Drama"},
    {"mal_id": 10, "name": "Fantasy"}
  ],
  "explicit_genres": [],
  "themes": [
    {"mal_id": 38, "name": "Military"}
  ],
  "demographics": [
    {"mal_id": 27, "name": "Shounen"}
  ],
  "studios": [
    {"mal_id": 4, "name": "Bones"}
  ],

  "score": 9.09,
  "scored_by": 1876543,
  "rank": 1,
  "popularity": 3,

  "source": "manga",
  "rating": "R - 17+ (violence & profanity)",
  "duration": 24,

  "fetched_at": "2025-11-09T10:30:00Z"
}
```

---

## 7. Deduplication Implementation

### HashSet-based Deduplication

```rust
use std::collections::HashSet;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct ScrapeStats {
    categories_processed: usize,
    categories_skipped: usize,
    raw_entries: usize,
    unique_anime: usize,
    duplicate_entries: usize,
    jobs_created: usize,
}

async fn scrape_all_categories(
    queue: &JobQueue,
    config: &Config,
) -> Result<ScrapeStats> {
    let mut anime_ids = HashSet::new();
    let mut stats = ScrapeStats::default();

    // Step 1: Auto-discover all categories
    println!("Discovering categories...");

    let all_genres = if config.process_genres {
        api::fetch_all_genres().await?
    } else {
        Vec::new()
    };

    let all_themes = if config.process_themes {
        api::fetch_all_themes().await?
    } else {
        Vec::new()
    };

    let all_demographics = if config.process_demographics {
        api::fetch_all_demographics().await?
    } else {
        Vec::new()
    };

    let all_studios = if config.process_studios {
        api::fetch_all_studios().await?
    } else {
        Vec::new()
    };

    // Step 2: Filter by min_category_size and exclusions
    let valid_genres = filter_categories(all_genres, config);
    let valid_themes = filter_categories(all_themes, config);
    let valid_demographics = filter_categories(all_demographics, config);
    let valid_studios = filter_categories(all_studios, config);

    println!("Categories to process:");
    println!("  Genres: {}", valid_genres.len());
    println!("  Themes: {}", valid_themes.len());
    println!("  Demographics: {}", valid_demographics.len());
    println!("  Studios: {}", valid_studios.len());
    println!("  Total: {}",
        valid_genres.len() + valid_themes.len() +
        valid_demographics.len() + valid_studios.len()
    );

    // Step 3: Fetch top 50 from each category
    for genre in valid_genres {
        println!("Fetching genre: {} ({} anime)", genre.name, genre.count);
        let top_50 = api::fetch_top_anime_by_genre(genre.mal_id, 50).await?;

        stats.raw_entries += top_50.len();
        for anime in top_50 {
            anime_ids.insert(anime.mal_id);
        }
        stats.categories_processed += 1;

        rate_limiter.wait().await; // Respect rate limits
    }

    // Repeat for themes, demographics, studios...
    // (similar code omitted for brevity)

    stats.unique_anime = anime_ids.len();
    stats.duplicate_entries = stats.raw_entries - stats.unique_anime;

    println!("\nDeduplication results:");
    println!("  Raw entries: {}", stats.raw_entries);
    println!("  Unique anime: {}", stats.unique_anime);
    println!("  Duplicates removed: {} ({:.1}%)",
        stats.duplicate_entries,
        (stats.duplicate_entries as f64 / stats.raw_entries as f64) * 100.0
    );

    // Step 4: Fetch detailed metadata and create jobs
    println!("\nFetching anime metadata...");
    for (index, mal_id) in anime_ids.iter().enumerate() {
        println!("[{}/{}] Fetching MAL ID: {}",
            index + 1, anime_ids.len(), mal_id
        );

        let metadata = api::fetch_anime_details(*mal_id).await?;

        // Insert/update anime in database
        let anime_id = queue.get_or_create_anime(*mal_id, metadata.clone())?;

        // Create jobs for each episode
        for episode in 1..=metadata.episodes {
            queue.enqueue(NewJob {
                anime_id,
                mal_id: *mal_id,
                anime_title: metadata.title.clone(),
                episode,
                priority: 0,
            })?;

            stats.jobs_created += 1;
        }

        rate_limiter.wait().await;
    }

    println!("\nScraping completed!");
    println!("  Total jobs created: {}", stats.jobs_created);

    Ok(stats)
}

fn filter_categories(
    categories: Vec<Category>,
    config: &Config,
) -> Vec<Category> {
    categories.into_iter()
        .filter(|cat| {
            // Filter by size
            if cat.count < config.min_category_size {
                return false;
            }

            // Filter by exclusions
            if config.exclude_categories.contains(&cat.name) {
                return false;
            }

            true
        })
        .collect()
}
```

---

## 8. CLI Interface

### Command-line Options

```bash
# Basic usage
mal-scraper --config config.toml --data-dir ./data

# Dry run (show what would be processed, don't write to DB)
mal-scraper --dry-run

# Clear cache and re-fetch everything
mal-scraper --clear-cache

# Only discover and show categories (don't fetch anime)
mal-scraper --discover-only

# Verbose logging
mal-scraper --verbose

# Custom output for stats
mal-scraper --stats-output stats.json

# Override config values
mal-scraper --min-category-size 30 --exclude "Hentai,Ecchi"
```

### CLI Implementation

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "mal-scraper")]
#[command(about = "MyAnimeList scraper for Zipf's law analysis")]
struct Args {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Data directory
    #[arg(short, long, default_value = "./data")]
    data_dir: PathBuf,

    /// Dry run (don't write to database)
    #[arg(long)]
    dry_run: bool,

    /// Clear cache before running
    #[arg(long)]
    clear_cache: bool,

    /// Only discover categories, don't fetch anime
    #[arg(long)]
    discover_only: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Output stats to JSON file
    #[arg(long)]
    stats_output: Option<PathBuf>,

    /// Override min category size
    #[arg(long)]
    min_category_size: Option<usize>,

    /// Categories to exclude (comma-separated)
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
}
```

---

## 9. Statistics Output

### Scraper Stats JSON

After completion, generate `data/mal_scraper_stats.json`:

```json
{
  "scrape_date": "2025-11-09T10:30:00Z",
  "version": "0.1.0",

  "config": {
    "min_category_size": 50,
    "process_genres": true,
    "process_explicit_genres": true,
    "process_themes": true,
    "process_demographics": true,
    "process_studios": true,
    "exclude_categories": []
  },

  "categories": {
    "genres_processed": 38,
    "genres_skipped": 4,
    "themes_processed": 45,
    "themes_skipped": 31,
    "demographics_processed": 4,
    "demographics_skipped": 1,
    "studios_processed": 87,
    "studios_skipped": 945,
    "total_processed": 174,
    "total_skipped": 981
  },

  "deduplication": {
    "raw_entries": 8700,
    "unique_anime": 1042,
    "duplicate_entries": 7658,
    "deduplication_rate": 0.880
  },

  "jobs": {
    "total_anime": 1042,
    "total_episodes": 15630,
    "jobs_created": 15630,
    "avg_episodes_per_anime": 15.0
  },

  "ranking_distribution": {
    "rank_1_100": 87,
    "rank_101_500": 312,
    "rank_501_1000": 245,
    "rank_1000_plus": 398
  },

  "file_sizes": {
    "category_cache_bytes": 8700000,
    "anime_metadata_cache_bytes": 10420000,
    "total_cache_bytes": 19120000
  },

  "performance": {
    "total_duration_seconds": 3245,
    "api_requests_made": 1390,
    "cache_hits": 156,
    "cache_misses": 1234,
    "retries": 12,
    "failed_requests": 0
  }
}
```

---

## 10. Error Handling

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ScraperError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    ConfigError(String),
}
```

### Logging

**Logging Framework**: `tracing` + `tracing-subscriber`

**Structured Logging** with contextual fields:

```rust
use tracing::{info, warn, error, debug, trace, instrument};

// Application startup
info!("Starting MAL scraper v{}", env!("CARGO_PKG_VERSION"));
info!(config_path = %config_path, "Loaded configuration");

// Category discovery
info!(
    genres = valid_genres.len(),
    themes = valid_themes.len(),
    studios = valid_studios.len(),
    total = total_categories,
    "Categories to process"
);

// Fetching with progress
debug!(
    category_type = "genre",
    category_name = %genre.name,
    count = genre.count,
    "Fetching top 50 anime"
);

// Individual anime processing
trace!(
    mal_id = anime.mal_id,
    title = %anime.title,
    rank = anime.rank,
    "Processing anime metadata"
);

// File operations with size tracking
info!(
    path = %cache_path,
    size_bytes = metadata.len(),
    "Wrote cache file"
);

// Warnings
warn!(
    category = %cat.name,
    count = cat.count,
    min_required = config.min_category_size,
    "Skipping category with insufficient anime"
);

// Errors with context
error!(
    mal_id = anime.mal_id,
    error = %e,
    retry_count = retry,
    "Failed to fetch anime metadata"
);

// Performance tracking
info!(
    duration_ms = elapsed.as_millis(),
    requests = request_count,
    cache_hits = cache_hits,
    "Scraping completed"
);

// Instrumented functions (auto-logs entry/exit)
#[instrument(skip(queue), fields(mal_id = %anime.mal_id))]
async fn fetch_anime_details(anime: &Anime, queue: &JobQueue) -> Result<()> {
    // Function body
}
```

**Log Levels Usage**:
- `ERROR`: API failures, database errors, unrecoverable errors
- `WARN`: Skipped categories, rate limiting, retries
- `INFO`: Major milestones (startup, category processing, completion), file I/O with sizes
- `DEBUG`: Individual API requests, cache operations, deduplication
- `TRACE`: Detailed anime metadata, every HTTP request/response

**File Size Tracking in Logs**:
```rust
// Cache file written
info!(
    file = "anime_5114_fullmetal.json",
    size_bytes = 10240,
    "Cached anime metadata"
);

// Stats summary
info!(
    category_cache_bytes = total_cat_bytes,
    anime_cache_bytes = total_anime_bytes,
    total_cache_bytes = total_cat_bytes + total_anime_bytes,
    "Cache statistics"
);
```

---

## 11. Configuration File (Final)

### `config.toml`

```toml
[mal_scraper]
# Auto-discovery
auto_discover_categories = true
min_category_size = 50

# Category types to process
process_genres = true
process_explicit_genres = true
process_themes = true
process_demographics = true
process_studios = true

# Manual exclusions
exclude_categories = []

[mal_scraper.api]
# Conservative rate limiting
requests_per_second = 2
requests_per_minute = 50
max_concurrent_requests = 1

# Timeouts
request_timeout_seconds = 30
connect_timeout_seconds = 10

# User agent
user_agent = "GDA2025-Zipf-Analysis/0.1 (Research Project)"

[mal_scraper.retry]
max_retries = 5
initial_delay_ms = 1000
max_delay_ms = 60000
backoff_factor = 2.0

# Retryable errors
retry_on = [
    "timeout",
    "network_error",
    "429_too_many_requests",
    "500_internal_server_error",
    "502_bad_gateway",
    "503_service_unavailable",
    "504_gateway_timeout"
]

# Non-retryable errors
no_retry_on = [
    "404_not_found",
    "403_forbidden",
    "401_unauthorized",
    "400_bad_request"
]

[mal_scraper.cache]
enabled = true
cache_dir = "data/cache/mal_cache"
never_expire = true
validate_on_startup = false
auto_refresh_on_missing = true
cache_format = "json"
compress_cache = false

[paths]
data_dir = "./data"
db_path = "./data/jobs.db"

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Format: pretty (human-readable), json (structured), compact
format = "pretty"

# Output targets
output_stdout = true
output_file = true

# File logging
log_file = "data/logs/mal_scraper.log"
log_rotation = "daily"          # daily, hourly, size-based, never
max_log_size_mb = 100           # For size-based rotation
max_log_files = 7               # Keep last 7 files

# Timestamp format
timestamp_format = "iso8601"    # iso8601, rfc3339, unix

# Include file/line numbers in logs (debug only)
include_location = false

# Module-specific log levels (override global level)
[logging.modules]
"mal_scraper::api" = "debug"         # More verbose for API
"mal_scraper::cache" = "info"
"reqwest" = "warn"                   # Less noise from reqwest
"rusqlite" = "warn"
```

---

## 12. Implementation Checklist

### Phase 1: Core Infrastructure
- [ ] Set up Cargo workspace structure
- [ ] Create `shared` crate with database models
- [ ] Implement SQLite schema with migrations
- [ ] Set up logging (tracing + tracing-subscriber)
- [ ] Implement config loading (config.toml)

### Phase 2: API Client
- [ ] Implement Jikan API v4 client (reqwest)
- [ ] Rate limiter (token bucket or leaky bucket)
- [ ] Retry logic with exponential backoff
- [ ] Error handling and logging
- [ ] API response models (serde)

### Phase 3: Cache System
- [ ] Cache directory structure setup
- [ ] Cache read/write operations
- [ ] Cache validation (file exists, not corrupted)
- [ ] Cache statistics tracking

### Phase 4: Category Discovery
- [ ] Fetch all genres
- [ ] Fetch all themes
- [ ] Fetch all demographics
- [ ] Fetch all studios
- [ ] Filter by min_category_size
- [ ] Apply exclusions

### Phase 5: Anime Fetching
- [ ] Fetch top 50 per category
- [ ] Deduplication (HashSet)
- [ ] Fetch detailed anime metadata
- [ ] Store in database (get_or_create_anime)

### Phase 6: Job Creation
- [ ] Create jobs for each episode
- [ ] Handle UNIQUE constraint violations
- [ ] Track job creation stats

### Phase 7: CLI & Stats
- [ ] CLI argument parsing (clap)
- [ ] Dry-run mode
- [ ] Stats output (JSON)
- [ ] Progress display

### Phase 8: Testing
- [ ] Unit tests for core functions
- [ ] Integration test with small dataset
- [ ] Mock Jikan API for testing
- [ ] Benchmark performance

---

## 13. Expected Timeline

### Development Time Estimate
- Phase 1 (Infrastructure): 2-3 days
- Phase 2 (API Client): 2-3 days
- Phase 3 (Cache): 1-2 days
- Phase 4 (Discovery): 1 day
- Phase 5 (Fetching): 2 days
- Phase 6 (Job Creation): 1 day
- Phase 7 (CLI): 1 day
- Phase 8 (Testing): 2-3 days

**Total**: ~12-18 days

### Runtime Estimate

With conservative rate limiting (2 req/s):
- Category discovery: ~100 categories ÷ 2 req/s = ~50 seconds
- Top 50 fetching: ~200 categories × 1 req ÷ 2 req/s = ~100 seconds
- Anime metadata: ~1000 anime × 1 req ÷ 2 req/s = ~500 seconds

**Total runtime**: ~11 minutes (first run, no cache)
**With cache**: <1 minute (subsequent runs)

---

*Last updated: 2025-11-09*
*Status: Specification finalized, ready for implementation*
