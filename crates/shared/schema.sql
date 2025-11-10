-- SQLite schema for GDA2025 Zipf's Law Analysis Project
-- This schema is embedded at compile time and run when creating a new database

-- Main jobs table
CREATE TABLE IF NOT EXISTS jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anime_id INTEGER NOT NULL,
    anime_title TEXT NOT NULL,
    anime_title_english TEXT,
    mal_id INTEGER,
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
    )) DEFAULT 'queued',
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
    duration_seconds INTEGER,

    -- File sizes (for statistics - preserved even after deletion)
    video_size_bytes INTEGER,
    audio_size_bytes INTEGER,
    transcript_size_bytes INTEGER,
    tokens_size_bytes INTEGER,

    -- Word/token counts
    word_count INTEGER,
    token_count INTEGER,

    -- Cleanup tracking
    video_deleted BOOLEAN DEFAULT 0,
    audio_deleted BOOLEAN DEFAULT 0,

    -- Priority and dependencies
    priority INTEGER DEFAULT 0,
    depends_on INTEGER,

    FOREIGN KEY (depends_on) REFERENCES jobs(id),
    FOREIGN KEY (anime_id) REFERENCES anime(id),

    -- Prevent duplicate jobs for same anime/episode
    UNIQUE(anime_id, episode)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_jobs_stage ON jobs(stage);
CREATE INDEX IF NOT EXISTS idx_jobs_anime_episode ON jobs(anime_id, episode);
CREATE INDEX IF NOT EXISTS idx_jobs_priority ON jobs(priority DESC, created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_updated_at ON jobs(updated_at);
CREATE INDEX IF NOT EXISTS idx_jobs_mal_id ON jobs(mal_id);

-- Anime metadata table
CREATE TABLE IF NOT EXISTS anime (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mal_id INTEGER UNIQUE NOT NULL,

    -- Titles
    title TEXT NOT NULL,
    title_english TEXT,
    title_japanese TEXT,
    title_synonyms TEXT,  -- JSON array

    -- Type and status
    type TEXT,            -- TV, Movie, OVA, etc.
    episodes_total INTEGER,
    status TEXT,          -- Finished Airing, Currently Airing, etc.

    -- Dates
    aired_from DATE,
    aired_to DATE,
    season TEXT,
    year INTEGER,

    -- Classification (JSON arrays)
    genres TEXT,           -- ["Action", "Adventure", ...]
    explicit_genres TEXT,  -- ["Boys Love", ...]
    themes TEXT,           -- ["School", "Military", ...]
    demographics TEXT,     -- ["Shounen", ...]
    studios TEXT,          -- ["Bones", ...]

    -- Scores and rankings
    score REAL,
    scored_by INTEGER,
    rank INTEGER,          -- Global ranking (for interval analysis)
    popularity INTEGER,

    -- Additional metadata
    source TEXT,
    rating TEXT,
    duration_minutes INTEGER,

    -- Processing stats
    episodes_processed INTEGER DEFAULT 0,
    processing_status TEXT DEFAULT 'pending' CHECK(processing_status IN (
        'pending', 'processing', 'completed', 'failed'
    )),

    -- Timestamps
    fetched_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_anime_mal_id ON anime(mal_id);
CREATE INDEX IF NOT EXISTS idx_anime_rank ON anime(rank);
CREATE INDEX IF NOT EXISTS idx_anime_score ON anime(score);
CREATE INDEX IF NOT EXISTS idx_anime_processing_status ON anime(processing_status);

-- Analysis results table
CREATE TABLE IF NOT EXISTS analysis_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    anime_id INTEGER NOT NULL,

    -- Zipf's law parameters
    zipf_alpha REAL,       -- Exponent
    zipf_constant REAL,    -- C constant
    r_squared REAL,        -- Goodness of fit

    -- Statistics
    total_words INTEGER,
    unique_words INTEGER,
    vocabulary_richness REAL,  -- unique/total

    -- Most frequent words (JSON array)
    top_10_words TEXT,
    top_50_words TEXT,

    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (anime_id) REFERENCES anime(id)
);

-- Worker status table (for TUI monitoring)
CREATE TABLE IF NOT EXISTS workers (
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

-- Triggers for automatic updated_at
CREATE TRIGGER IF NOT EXISTS update_jobs_timestamp
AFTER UPDATE ON jobs
BEGIN
    UPDATE jobs SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_anime_timestamp
AFTER UPDATE ON anime
BEGIN
    UPDATE anime SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
