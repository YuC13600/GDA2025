# Technical Details: Job Queue and File Structure

This document provides detailed specifications for the job queue system and data file organization.

## Job Queue Design (SQLite)

### Database Schema

```sql
-- Main jobs table
CREATE TABLE jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anime_id INTEGER NOT NULL,
    anime_title TEXT NOT NULL,
    anime_title_english TEXT,
    mal_id INTEGER,              -- MyAnimeList ID
    episode INTEGER NOT NULL,
    season INTEGER,
    year INTEGER,

    -- Job status
    stage TEXT NOT NULL CHECK(stage IN (
        'queued',
        'downloading',
        'downloaded',
        'transcribing',
        'transcribed',
        'tokenizing',
        'tokenized',
        'analyzing',
        'complete',
        'failed'
    )),
    progress REAL DEFAULT 0.0 CHECK(progress >= 0.0 AND progress <= 1.0),

    -- Timestamps
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,

    -- Error handling
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,

    -- File paths (relative to data directory)
    video_path TEXT,
    transcript_path TEXT,
    tokens_path TEXT,
    analysis_path TEXT,

    -- Metadata
    duration_seconds INTEGER,    -- Video duration

    -- File sizes (for statistics - preserved even after deletion)
    video_size_bytes INTEGER,    -- Video file size (kept for stats)
    audio_size_bytes INTEGER,    -- Audio file size (kept for stats)
    transcript_size_bytes INTEGER,  -- Transcript file size
    tokens_size_bytes INTEGER,   -- Tokens file size

    -- Word/token counts
    word_count INTEGER,          -- After tokenization
    token_count INTEGER,         -- Total tokens (including particles)

    -- Cleanup tracking
    video_deleted BOOLEAN DEFAULT 0,
    audio_deleted BOOLEAN DEFAULT 0,

    -- Priority and dependencies
    priority INTEGER DEFAULT 0,  -- Higher = more important
    depends_on INTEGER,          -- Foreign key to another job

    FOREIGN KEY (depends_on) REFERENCES jobs(id),
    FOREIGN KEY (anime_id) REFERENCES anime(id),

    -- CRITICAL: Prevent duplicate jobs for same anime/episode
    UNIQUE(anime_id, episode)
);

-- Indexes for efficient queries
CREATE INDEX idx_stage ON jobs(stage);
CREATE INDEX idx_anime_episode ON jobs(anime_id, episode);
CREATE INDEX idx_priority ON jobs(priority DESC, created_at);
CREATE INDEX idx_updated_at ON jobs(updated_at);
CREATE INDEX idx_mal_id ON jobs(mal_id);

-- Anime metadata table
CREATE TABLE anime (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mal_id INTEGER UNIQUE NOT NULL,

    -- Titles
    title TEXT NOT NULL,
    title_english TEXT,
    title_japanese TEXT,
    title_synonyms TEXT,              -- JSON array

    -- Type and status
    type TEXT,                         -- TV, Movie, OVA, etc.
    episodes_total INTEGER,
    status TEXT,                       -- Finished Airing, Currently Airing, etc.

    -- Dates
    aired_from DATE,
    aired_to DATE,
    season TEXT,
    year INTEGER,

    -- Classification (JSON arrays)
    genres TEXT,                       -- ["Action", "Adventure", ...]
    explicit_genres TEXT,              -- ["Boys Love", ...]
    themes TEXT,                       -- ["School", "Military", ...]
    demographics TEXT,                 -- ["Shounen", ...]
    studios TEXT,                      -- ["Bones", ...]

    -- Scores and rankings
    score REAL,
    scored_by INTEGER,
    rank INTEGER,                      -- Global ranking (for interval analysis)
    popularity INTEGER,

    -- Additional metadata
    source TEXT,
    rating TEXT,
    duration_minutes INTEGER,

    -- Processing stats
    episodes_processed INTEGER DEFAULT 0,
    processing_status TEXT DEFAULT 'pending',

    -- Timestamps
    fetched_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_mal_id ON anime(mal_id);
CREATE INDEX idx_rank ON anime(rank);
CREATE INDEX idx_score ON anime(score);
CREATE INDEX idx_processing_status ON anime(processing_status);

-- Triggers for automatic updated_at
CREATE TRIGGER update_anime_timestamp
AFTER UPDATE ON anime
BEGIN
    UPDATE anime SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- Analysis results table
CREATE TABLE analysis_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anime_id INTEGER NOT NULL,

    -- Zipf's law parameters
    zipf_alpha REAL,             -- Exponent
    zipf_constant REAL,          -- C constant
    r_squared REAL,              -- Goodness of fit

    -- Statistics
    total_words INTEGER,
    unique_words INTEGER,
    vocabulary_richness REAL,    -- unique/total

    -- Most frequent words (JSON array)
    top_10_words TEXT,
    top_50_words TEXT,

    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (anime_id) REFERENCES anime(id)
);

-- Worker status table (for TUI monitoring)
CREATE TABLE workers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    worker_type TEXT NOT NULL CHECK(worker_type IN (
        'downloader',
        'transcriber',
        'tokenizer',
        'analyzer'
    )),
    status TEXT CHECK(status IN ('idle', 'busy', 'error')),
    current_job_id INTEGER,
    last_heartbeat TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (current_job_id) REFERENCES jobs(id)
);

-- Anime selection cache (Claude Haiku selections)
-- Caches which anime to download for each MAL ID to avoid repeated API calls
CREATE TABLE anime_selection_cache (
    mal_id INTEGER PRIMARY KEY,
    anime_title TEXT NOT NULL,
    search_query TEXT NOT NULL,
    selected_index INTEGER NOT NULL,      -- 1-based index from candidates list
    selected_title TEXT NOT NULL,         -- The title that was selected
    confidence TEXT NOT NULL CHECK(confidence IN ('high', 'medium', 'low', 'no_candidates')),
    reason TEXT,
    mal_episodes INTEGER,                 -- Episode count from MAL metadata
    selected_episodes INTEGER,            -- Episode count from selected anime
    episode_match TEXT CHECK(episode_match IN ('exact', 'close', 'acceptable', 'mismatch', 'unknown', NULL)),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (mal_id) REFERENCES anime(mal_id)
);

CREATE INDEX idx_selection_cache_confidence ON anime_selection_cache(confidence);
CREATE INDEX idx_selection_cache_episode_match ON anime_selection_cache(episode_match);

-- Triggers for automatic updated_at
CREATE TRIGGER update_jobs_timestamp
AFTER UPDATE ON jobs
BEGIN
    UPDATE jobs SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
```

### Job Queue API (Rust)

```rust
// shared/src/queue.rs
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

pub struct JobQueue {
    conn: Arc<Mutex<Connection>>,
}

impl JobQueue {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Get or create anime entry (deduplication)
    pub fn get_or_create_anime(&self, mal_id: u32, metadata: AnimeMetadata) -> Result<i64> {
        let conn = self.conn.lock().unwrap();

        // Try to find existing anime by MAL ID
        let existing: Option<i64> = conn.query_row(
            "SELECT id FROM anime WHERE mal_id = ?1",
            params![mal_id],
            |row| row.get(0)
        ).optional()?;

        if let Some(id) = existing {
            // Anime already exists, update categories if new ones found
            conn.execute(
                "UPDATE anime SET
                    genre = json_array_append(genre, ?, ?),
                    theme = json_array_append(theme, ?, ?),
                    studio = json_array_append(studio, ?, ?)
                 WHERE id = ?",
                params![metadata.genres, metadata.themes, metadata.studios, id],
            )?;
            Ok(id)
        } else {
            // Insert new anime
            conn.execute(
                "INSERT INTO anime (mal_id, title, title_english, title_japanese,
                                   genre, theme, studio, score, rank, episodes_total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    mal_id,
                    metadata.title,
                    metadata.title_english,
                    metadata.title_japanese,
                    serde_json::to_string(&metadata.genres)?,
                    serde_json::to_string(&metadata.themes)?,
                    serde_json::to_string(&metadata.studios)?,
                    metadata.score,
                    metadata.rank,
                    metadata.episodes_total,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    /// Enqueue a new job (with deduplication)
    pub fn enqueue(&self, job: NewJob) -> Result<i64> {
        let conn = self.conn.lock().unwrap();

        // Use INSERT OR IGNORE to handle duplicates gracefully
        match conn.execute(
            "INSERT INTO jobs (anime_id, anime_title, episode, stage, priority, mal_id)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?5)",
            params![job.anime_id, job.anime_title, job.episode, job.priority, job.mal_id],
        ) {
            Ok(_) => Ok(conn.last_insert_rowid()),
            Err(rusqlite::Error::SqliteFailure { code: rusqlite::ErrorCode::ConstraintViolation, .. }) => {
                // Job already exists, return existing job ID
                let existing_id: i64 = conn.query_row(
                    "SELECT id FROM jobs WHERE anime_id = ?1 AND episode = ?2",
                    params![job.anime_id, job.episode],
                    |row| row.get(0)
                )?;
                Ok(existing_id)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Dequeue next job for a specific stage (atomic operation)
    pub fn dequeue(&self, from_stage: &str, to_stage: &str) -> Result<Option<Job>> {
        let conn = self.conn.lock().unwrap();

        // Atomic update: mark job as in-progress
        conn.execute(
            "UPDATE jobs SET stage = ?1, started_at = CURRENT_TIMESTAMP
             WHERE id = (
                 SELECT id FROM jobs
                 WHERE stage = ?2
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
             )",
            params![to_stage, from_stage],
        )?;

        // Fetch the job we just updated
        let mut stmt = conn.prepare(
            "SELECT * FROM jobs WHERE stage = ?1 ORDER BY updated_at DESC LIMIT 1"
        )?;

        let job = stmt.query_row(params![to_stage], |row| {
            Ok(Job {
                id: row.get(0)?,
                anime_id: row.get(1)?,
                anime_title: row.get(2)?,
                episode: row.get(3)?,
                stage: row.get(4)?,
                video_path: row.get(5)?,
                // ... other fields
            })
        }).optional()?;

        Ok(job)
    }

    /// Update job progress
    pub fn update_progress(&self, job_id: i64, progress: f64, stage: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        if let Some(new_stage) = stage {
            conn.execute(
                "UPDATE jobs SET progress = ?1, stage = ?2 WHERE id = ?3",
                params![progress, new_stage, job_id],
            )?;
        } else {
            conn.execute(
                "UPDATE jobs SET progress = ?1 WHERE id = ?2",
                params![progress, job_id],
            )?;
        }
        Ok(())
    }

    /// Mark file as deleted
    pub fn mark_file_deleted(&self, job_id: i64, file_type: FileType) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let column = match file_type {
            FileType::Video => "video_deleted",
            FileType::Audio => "audio_deleted",
        };
        conn.execute(
            &format!("UPDATE jobs SET {} = 1 WHERE id = ?1", column),
            params![job_id],
        )?;
        Ok(())
    }

    /// Mark job as failed with error message
    pub fn fail_job(&self, job_id: i64, error: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE jobs
             SET stage = 'failed',
                 error_message = ?1,
                 retry_count = retry_count + 1
             WHERE id = ?2",
            params![error, job_id],
        )?;
        Ok(())
    }

    /// Get all jobs for TUI display
    pub fn get_all_jobs(&self) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT * FROM jobs ORDER BY priority DESC, created_at ASC"
        )?;

        let jobs = stmt.query_map([], |row| {
            Ok(Job { /* map all fields */ })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Retry failed jobs (reset to queued)
    pub fn retry_failed(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE jobs
             SET stage = 'queued',
                 error_message = NULL,
                 progress = 0.0
             WHERE stage = 'failed' AND retry_count < max_retries",
            [],
        )?;
        Ok(updated)
    }

    /// Get cached anime selection
    pub fn get_selection(&self, mal_id: u32) -> Result<Option<AnimeSelection>> {
        let conn = self.conn.lock().unwrap();
        let selection = conn.query_row(
            "SELECT selected_index, selected_title, confidence, reason
             FROM anime_selection_cache WHERE mal_id = ?1",
            params![mal_id],
            |row| Ok(AnimeSelection {
                selected_index: row.get(0)?,
                selected_title: row.get(1)?,
                confidence: row.get(2)?,
                reason: row.get(3)?,
            })
        ).optional()?;
        Ok(selection)
    }

    /// Cache anime selection
    pub fn cache_selection(&self, mal_id: u32, selection: AnimeSelection) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO anime_selection_cache
             (mal_id, anime_title, search_query, selected_index, selected_title, confidence, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                mal_id,
                selection.anime_title,
                selection.search_query,
                selection.selected_index,
                selection.selected_title,
                selection.confidence,
                selection.reason,
            ],
        )?;
        Ok(())
    }
}

pub enum FileType {
    Video,
    Audio,
}

pub struct AnimeSelection {
    pub selected_index: i32,
    pub selected_title: String,
    pub confidence: String,  // "high", "medium", "low", or "no_candidates"
    pub reason: String,
    pub anime_title: String,
    pub search_query: String,
    pub mal_episodes: Option<i32>,        // Episode count from MAL
    pub selected_episodes: Option<i32>,   // Episode count from selected anime
    pub episode_match: Option<String>,    // "exact", "close", "acceptable", "mismatch", "unknown"
}
```

### Deduplication Strategy

**Problem**: Multiple categories (Genres, Themes, Studios) may contain the same anime.

**Example**:
- "Fullmetal Alchemist: Brotherhood" appears in:
  - Genre: Action (rank 1), Adventure (rank 1), Fantasy (rank 1)
  - Theme: Military (rank 1)
  - Studio: Bones (rank 5)

**Solution**:

1. **MAL Scraper deduplication workflow**:
```rust
// mal-scraper/src/main.rs

async fn scrape_all_categories(queue: &JobQueue) -> Result<()> {
    let mut anime_ids = HashSet::new();

    // Scrape all categories
    for genre in GENRES {
        let anime_list = fetch_top_50_by_genre(genre).await?;
        for anime in anime_list {
            anime_ids.insert(anime.mal_id);
        }
    }

    for theme in THEMES {
        let anime_list = fetch_top_50_by_theme(theme).await?;
        for anime in anime_list {
            anime_ids.insert(anime.mal_id);
        }
    }

    for studio in STUDIOS {
        let anime_list = fetch_top_50_by_studio(studio).await?;
        for anime in anime_list {
            anime_ids.insert(anime.mal_id);
        }
    }

    // Now process unique anime only
    println!("Found {} unique anime across all categories", anime_ids.len());

    for mal_id in anime_ids {
        let metadata = fetch_anime_details(mal_id).await?;
        let anime_id = queue.get_or_create_anime(mal_id, metadata)?;

        // Enqueue jobs for all episodes
        for episode in 1..=metadata.episodes_total {
            queue.enqueue(NewJob {
                anime_id,
                mal_id,
                anime_title: metadata.title.clone(),
                episode,
                priority: 0,
            })?;
        }
    }

    Ok(())
}
```

2. **Database-level deduplication**:
   - `anime.mal_id` is UNIQUE → prevents duplicate anime entries
   - `jobs(anime_id, episode)` is UNIQUE → prevents duplicate episode jobs

3. **Deduplication guarantees**:
   - ✅ Same anime in multiple categories → downloaded once
   - ✅ Same episode → processed once
   - ✅ Failed jobs can be retried without creating duplicates

**Result**: If 50 genres × 50 anime = 2500 entries, but only ~500 unique anime → **5x reduction** in downloads!

---

### Anime Selection Strategy

**Problem**: AllAnime search returns multiple results for each anime (main series, specials, OVAs, recaps). Simple auto-selection (first result) often downloads wrong content.

**Example**:
- Search for "ACCA: 13-ku Kansatsu-ka" returns:
  1. "ACCA: 13-ku Kansatsu-ka Specials (6 eps)" ❌
  2. "ACCA: 13-ku Kansatsu-ka - Regards (1 eps)" ❌
  3. "ACCA: 13-ku Kansatsu-ka (12 eps)" ✅ Correct!

**Solution**: Claude Haiku pre-selection

**Workflow**:

1. **anime-selector (Phase 3)** - Run once before downloading:
```rust
// anime-selector/src/main.rs

async fn select_anime(mal_id: u32, metadata: AnimeMetadata) -> Result<()> {
    // Check cache first
    if let Some(selection) = queue.get_selection(mal_id)? {
        println!("Using cached selection for {}", metadata.title);
        return Ok(());
    }

    // Fetch candidates from AllAnime
    let candidates = get_anime_candidates(&metadata.title)?;

    // Use Claude Haiku to select best match
    let selection = claude_select(metadata, candidates).await?;

    // Cache the result
    queue.cache_selection(mal_id, selection)?;

    Ok(())
}

fn get_anime_candidates(title: &str) -> Result<Vec<String>> {
    // Call scripts/get_anime_candidates.sh
    let output = Command::new("zsh")
        .args(&["scripts/get_anime_candidates.sh", title])
        .output()?;

    let candidates: Vec<String> = serde_json::from_slice(&output.stdout)?;
    Ok(candidates)
}

async fn claude_select(metadata: AnimeMetadata, candidates: Vec<String>) -> Result<AnimeSelection> {
    // Call scripts/select_anime.py via Python
    let candidates_json = serde_json::to_string(&candidates)?;

    let output = Command::new("python3")
        .args(&[
            "scripts/select_anime.py",
            "--mal-title", &metadata.title,
            "--episodes", &metadata.episodes.to_string(),
            "--year", &metadata.year.to_string(),
            "--anime-type", &metadata.anime_type,
            "--candidates", &candidates_json,
        ])
        .output()?;

    let result: SelectionResult = serde_json::from_slice(&output.stdout)?;

    Ok(AnimeSelection {
        selected_index: result.index,
        selected_title: candidates[result.index - 1].clone(),
        confidence: result.confidence,
        reason: result.reason,
        anime_title: metadata.title,
        search_query: format!("{}#{}", metadata.title, result.index),
    })
}
```

2. **anime-downloader (Phase 4)** - Reads cached selections:
```rust
// anime-downloader/src/main.rs

async fn download_episode(job: Job, queue: &JobQueue) -> Result<()> {
    // Get cached selection
    let selection = queue.get_selection(job.mal_id)?
        .ok_or_else(|| anyhow!("No cached selection for MAL ID {}", job.mal_id))?;

    // Use ani-cli with cached index
    let output = Command::new("ani-cli")
        .args(&[
            "-S", &selection.selected_index.to_string(),  // Use cached index
            "-e", &format!("{}", job.episode),
            &job.anime_title
        ])
        .output()?;

    // ... handle download
}
```

**Benefits**:
- **Separation of concerns**: Selection and downloading are independent phases
- **Cost-effective**: Each anime selected once, results cached
- **Manual review**: Low-confidence selections can be reviewed before downloading
- **Fail-safe**: If selection fails, job can be retried without re-selecting

**Cost Analysis**:
- Claude Haiku: ~$0.25 per million input tokens, ~$1.25 per million output tokens
- Estimated input: ~100 tokens per selection (MAL metadata + candidates)
- Estimated output: ~50 tokens per selection (JSON response)
- Cost per selection: ~$0.000225
- Total for 171,851 anime: ~$38.67

---

## Data File Structure

### Directory Organization

```
data/
├── videos/                      # Downloaded videos (TEMPORARY)
│   ├── <anime_id>/
│   │   ├── metadata.json        # Anime metadata from MAL (PERMANENT)
│   │   └── episodes/
│   │       ├── ep001.mkv        # Auto-deleted after tokenization
│   │       ├── ep002.mkv
│   │       └── ...
│   └── ...
│
├── audio/                       # Extracted audio (TEMPORARY)
│   ├── <anime_id>/
│   │   ├── ep001.wav            # 16kHz mono WAV
│   │   └── ...                  # Auto-deleted after transcription
│   └── ...
│
├── transcripts/                 # Whisper output (PERMANENT)
│   ├── <anime_id>/
│   │   ├── ep001.txt            # Raw transcript
│   │   ├── ep001.json           # With timestamps and metadata
│   │   └── ...
│   └── ...
│
├── tokens/                      # Tokenized output (PERMANENT)
│   ├── <anime_id>/
│   │   ├── ep001_tokens.json    # Full tokenization
│   │   ├── ep001_freq.csv       # Word frequency list
│   │   └── ...
│   └── ...
│
├── analysis/                    # Analysis results (PERMANENT)
│   ├── per_anime/
│   │   ├── <anime_id>/
│   │   │   ├── word_freq.csv            # Aggregated frequencies
│   │   │   ├── zipf_params.json         # Fitted parameters
│   │   │   ├── zipf_plot.html           # Interactive plot
│   │   │   └── statistics.json          # Summary stats
│   │   └── ...
│   │
│   ├── aggregated/
│   │   ├── all_anime_freq.csv           # Global word frequencies
│   │   ├── by_genre/
│   │   │   ├── action_freq.csv
│   │   │   ├── romance_freq.csv
│   │   │   └── ...
│   │   ├── by_studio/
│   │   │   └── <studio_name>_freq.csv
│   │   └── comparison.html              # Comparative plots
│   │
│   └── reports/
│       ├── zipf_validation.pdf
│       └── final_report.html
│
├── models/                      # Whisper models (downloaded once)
│   ├── ggml-base.bin
│   ├── ggml-small.bin
│   └── ggml-large-v3.bin
│
├── cache/                       # Temporary cache
│   ├── mal_cache/               # Cached MAL API responses
│   │   └── anime_<mal_id>.json
│   └── vibrato_dict/            # Tokenizer dictionaries
│       └── ipadic/
│
└── jobs.db                      # SQLite job queue database
```

### File Format Specifications

#### 1. metadata.json (per anime)
```json
{
  "mal_id": 5114,
  "title": "Fullmetal Alchemist: Brotherhood",
  "title_english": "Fullmetal Alchemist: Brotherhood",
  "title_japanese": "鋼の錬金術師 FULLMETAL ALCHEMIST",
  "genres": ["Action", "Adventure", "Drama", "Fantasy"],
  "themes": ["Military"],
  "studios": ["Bones"],
  "score": 9.09,
  "rank": 1,
  "episodes": 64,
  "season": "spring",
  "year": 2009,
  "fetched_at": "2025-11-06T12:00:00Z"
}
```

#### 2. Transcript with Timestamps (ep001.json)
```json
{
  "anime_id": 5114,
  "episode": 1,
  "duration_seconds": 1440,
  "language": "ja",
  "model": "whisper-base",
  "segments": [
    {
      "id": 0,
      "start": 0.0,
      "end": 2.5,
      "text": "これは錬金術という科学の物語だ",
      "confidence": 0.89
    },
    {
      "id": 1,
      "start": 2.5,
      "end": 5.2,
      "text": "等価交換という原則がある",
      "confidence": 0.92
    }
  ],
  "transcribed_at": "2025-11-06T13:30:00Z"
}
```

#### 3. Tokenized Output (ep001_tokens.json)
```json
{
  "anime_id": 5114,
  "episode": 1,
  "total_tokens": 5234,
  "unique_tokens": 1823,
  "tokenizer": "vibrato",
  "dictionary": "ipadic",
  "tokens": [
    {
      "surface": "これ",
      "pos": "代名詞",
      "pos_detail": "一般",
      "reading": "コレ",
      "base_form": "これ"
    },
    {
      "surface": "は",
      "pos": "助詞",
      "pos_detail": "係助詞",
      "reading": "ハ",
      "base_form": "は"
    },
    {
      "surface": "錬金術",
      "pos": "名詞",
      "pos_detail": "一般",
      "reading": "レンキンジュツ",
      "base_form": "錬金術"
    }
  ],
  "tokenized_at": "2025-11-06T14:00:00Z"
}
```

#### 4. Word Frequency CSV (ep001_freq.csv)
```csv
word,count,pos,reading
の,245,助詞,ノ
は,198,助詞,ハ
錬金術,87,名詞,レンキンジュツ
だ,65,助動詞,ダ
を,58,助詞,ヲ
```

#### 5. Zipf Analysis Parameters (zipf_params.json)
```json
{
  "anime_id": 5114,
  "anime_title": "Fullmetal Alchemist: Brotherhood",
  "episodes_analyzed": 64,
  "total_words": 334520,
  "unique_words": 12483,
  "vocabulary_richness": 0.0373,

  "zipf_fit": {
    "alpha": 1.02,
    "constant": 8234.5,
    "r_squared": 0.987,
    "method": "log-log linear regression"
  },

  "top_words": [
    {"word": "の", "count": 15678, "rank": 1, "frequency": 0.0469},
    {"word": "は", "count": 12456, "rank": 2, "frequency": 0.0372},
    {"word": "を", "count": 9823, "rank": 3, "frequency": 0.0294}
  ],

  "analyzed_at": "2025-11-06T15:00:00Z"
}
```

---

## File Naming Conventions (Rust)

```rust
// shared/src/paths.rs

use std::path::{Path, PathBuf};

pub struct DataPaths {
    pub root: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    // Video paths (TEMPORARY - auto-deleted)
    pub fn video_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("videos").join(anime_id.to_string()).join("episodes")
    }

    pub fn video_file(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.video_dir(anime_id).join(format!("ep{:03}.mkv", episode))
    }

    // Audio paths (TEMPORARY - auto-deleted)
    pub fn audio_file(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.root.join("audio")
            .join(anime_id.to_string())
            .join(format!("ep{:03}.wav", episode))
    }

    // Transcript paths (PERMANENT)
    pub fn transcript_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("transcripts").join(anime_id.to_string())
    }

    pub fn transcript_txt(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.transcript_dir(anime_id).join(format!("ep{:03}.txt", episode))
    }

    pub fn transcript_json(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.transcript_dir(anime_id).join(format!("ep{:03}.json", episode))
    }

    // Token paths (PERMANENT)
    pub fn tokens_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("tokens").join(anime_id.to_string())
    }

    pub fn tokens_json(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.tokens_dir(anime_id).join(format!("ep{:03}_tokens.json", episode))
    }

    pub fn freq_csv(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.tokens_dir(anime_id).join(format!("ep{:03}_freq.csv", episode))
    }

    // Analysis paths (PERMANENT)
    pub fn analysis_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("analysis").join("per_anime").join(anime_id.to_string())
    }

    pub fn zipf_params(&self, anime_id: u32) -> PathBuf {
        self.analysis_dir(anime_id).join("zipf_params.json")
    }

    pub fn zipf_plot(&self, anime_id: u32) -> PathBuf {
        self.analysis_dir(anime_id).join("zipf_plot.html")
    }

    // Metadata
    pub fn anime_metadata(&self, anime_id: u32) -> PathBuf {
        self.root.join("videos")
            .join(anime_id.to_string())
            .join("metadata.json")
    }

    // Database
    pub fn jobs_db(&self) -> PathBuf {
        self.root.join("jobs.db")
    }
}
```

---

## Automatic Cleanup Strategy (Optimized for 250GB Peak)

### Aggressive Cleanup Workflow

**Goal**: Keep peak disk usage under 250GB by deleting files as soon as possible.

**Strategy**: Delete video immediately after transcription (don't wait for tokenization)

```rust
// transcriber/src/main.rs

async fn process_job(job: Job, queue: &JobQueue, paths: &DataPaths) -> Result<()> {
    let video_path = paths.video_file(job.anime_id, job.episode);
    let audio_path = paths.audio_file(job.anime_id, job.episode);
    let transcript_path = paths.transcript_json(job.anime_id, job.episode);

    // Transcribe
    transcribe_audio(&audio_path, &transcript_path).await?;

    // Update job status
    queue.update_progress(job.id, 1.0, Some("transcribed"))?;

    // AGGRESSIVE CLEANUP: Delete BOTH audio AND video after transcription
    if audio_path.exists() {
        std::fs::remove_file(&audio_path)?;
        queue.mark_file_deleted(job.id, FileType::Audio)?;
        println!("Deleted audio: {:?}", audio_path);
    }

    if video_path.exists() {
        std::fs::remove_file(&video_path)?;
        queue.mark_file_deleted(job.id, FileType::Video)?;
        println!("Deleted video: {:?}", video_path);
    }

    Ok(())
}
```

```rust
// tokenizer/src/main.rs

async fn process_job(job: Job, queue: &JobQueue, paths: &DataPaths) -> Result<()> {
    let transcript_path = paths.transcript_json(job.anime_id, job.episode);
    let tokens_path = paths.tokens_json(job.anime_id, job.episode);

    // Tokenize (video already deleted by transcriber)
    tokenize_transcript(&transcript_path, &tokens_path).await?;

    // Update job status
    queue.update_progress(job.id, 1.0, Some("tokenized"))?;

    // No cleanup needed - video already deleted

    Ok(())
}
```

### Error Handling for Cleanup

```rust
// If job fails, keep files for retry
async fn handle_job_error(job: Job, error: &str, queue: &JobQueue, paths: &DataPaths) -> Result<()> {
    queue.fail_job(job.id, error)?;

    // Do NOT delete files - keep for retry
    println!("Job {} failed, keeping files for retry: {}", job.id, error);

    Ok(())
}
```

---

## Disk Space Estimation (Optimized for 250GB Peak)

### With Deduplication and Aggressive Cleanup

Assuming **~500 unique anime** (after deduplication from all categories) × 18 episodes = **9000 episodes total**

| File Type | Size per Episode | Lifecycle | Peak Storage | Final Storage |
|-----------|------------------|-----------|--------------|---------------|
| Video (MKV) | 500 MB | Deleted after transcription | **~50 episodes × 500MB = 25 GB** | 0 GB |
| Audio (WAV) | 50 MB | Deleted after transcription | **~50 episodes × 50MB = 2.5 GB** | 0 GB |
| Transcript (JSON) | 100 KB | Permanent | ~900 MB | ~900 MB |
| Tokens (JSON) | 500 KB | Permanent | ~4.5 GB | ~4.5 GB |
| Word Freq (CSV) | 200 KB | Permanent | ~1.8 GB | ~1.8 GB |
| Analysis Results | 1 MB per anime | Permanent | ~500 MB | ~500 MB |
| **Peak Usage** | | | **~35 GB** | |
| **Long-term Storage** | | | | **~7.7 GB** |

**Key Optimization**: Video deleted immediately after transcription → only ~50 episodes in flight at peak

### Disk Space Management Strategy

**1. Controlled Concurrency** (limits simultaneous downloads):
```rust
// scheduler-tui/config.toml
[processing]
# Limit concurrent jobs to control disk usage
max_concurrent_downloads = 50        # 50 episodes × 500MB = 25 GB peak
max_concurrent_transcriptions = 4    # GPU/CPU bound
max_concurrent_tokenizations = 8     # CPU bound

# Cleanup happens immediately after transcription
aggressive_cleanup = true            # Delete video after transcription
```

**2. Processing Strategy**:
- Download up to 50 episodes
- Transcribe each episode → **immediately delete video and audio**
- Tokenize from saved transcripts (video already gone)
- Repeat for next batch

**3. Peak Usage Breakdown** (worst case):
```
Videos (50 episodes):        25.0 GB
Audio (50 episodes):          2.5 GB
Transcripts (accumulated):    0.9 GB
Tokens (accumulated):         4.5 GB
Analysis (accumulated):       0.5 GB
Database + cache:             0.5 GB
─────────────────────────────────────
Total Peak:                  ~34 GB ✅ Well under 250 GB target!
```

**4. TUI Disk Monitor**:
```rust
// Display in TUI
Storage Usage:
  Current: 34 GB / 250 GB (13%) ✅

  Temporary Files:
    Videos (in queue): 25.0 GB (50 episodes)
    Audio (processing): 2.5 GB

  Permanent Data:
    Transcripts: 900 MB (9000 episodes)
    Tokens: 4.5 GB
    Analysis: 500 MB

  Processing Rate:
    Transcriptions/hour: 120 episodes
    Est. time to completion: 75 hours
```

**5. Deduplication Impact**:
- Without deduplication: 50 categories × 50 anime = 2500 anime → **~60 GB peak** (still OK)
- With deduplication: ~500 unique anime → **~34 GB peak** (optimized!)
- **Storage savings**: 80% reduction in downloads

### Scaling to Higher Concurrency

If you want faster processing (more parallel jobs):

| Concurrent Downloads | Video Storage | Audio Storage | Total Peak | Time to Complete |
|---------------------|---------------|---------------|------------|------------------|
| 50 episodes | 25 GB | 2.5 GB | ~35 GB | ~75 hours |
| 100 episodes | 50 GB | 5 GB | ~60 GB | ~37 hours |
| 200 episodes | 100 GB | 10 GB | ~115 GB | ~18 hours |
| 400 episodes | 200 GB | 20 GB | ~225 GB | ~9 hours |

**Recommendation**: Start with 50 concurrent downloads (~35 GB peak), increase if you need faster processing and have disk space available.

---

*Last updated: 2025-11-13*
*See PLAN.md for overall implementation plan*
