//! Data models for the project.
//!
//! This module defines all the data structures used throughout the pipeline,
//! including anime metadata, job information, and analysis results.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Anime metadata from MyAnimeList
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anime {
    pub id: Option<i64>,          // Database ID (None before insertion)
    pub mal_id: u32,              // MyAnimeList ID

    // Titles
    pub title: String,
    pub title_english: Option<String>,
    pub title_japanese: Option<String>,
    pub title_synonyms: Vec<String>,

    // Type and status
    pub anime_type: Option<String>,  // TV, Movie, OVA, etc.
    pub episodes_total: Option<u32>,
    pub status: Option<String>,

    // Dates
    pub aired_from: Option<NaiveDate>,
    pub aired_to: Option<NaiveDate>,
    pub season: Option<String>,
    pub year: Option<i32>,

    // Classifications (stored as JSON arrays in database)
    pub genres: Vec<String>,
    pub explicit_genres: Vec<String>,
    pub themes: Vec<String>,
    pub demographics: Vec<String>,
    pub studios: Vec<String>,

    // Scores and rankings
    pub score: Option<f64>,
    pub scored_by: Option<u32>,
    pub rank: Option<u32>,
    pub popularity: Option<u32>,

    // Additional metadata
    pub source: Option<String>,
    pub rating: Option<String>,
    pub duration_minutes: Option<u32>,

    // Processing status
    pub episodes_processed: u32,
    pub processing_status: ProcessingStatus,

    // Timestamps
    pub fetched_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Processing status for anime
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl std::fmt::Display for ProcessingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingStatus::Pending => write!(f, "pending"),
            ProcessingStatus::Processing => write!(f, "processing"),
            ProcessingStatus::Completed => write!(f, "completed"),
            ProcessingStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for ProcessingStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ProcessingStatus::Pending),
            "processing" => Ok(ProcessingStatus::Processing),
            "completed" => Ok(ProcessingStatus::Completed),
            "failed" => Ok(ProcessingStatus::Failed),
            _ => Err(anyhow::anyhow!("Invalid processing status: {}", s)),
        }
    }
}

/// Job stage in the processing pipeline
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStage {
    Queued,
    Downloading,
    Downloaded,
    Transcribing,
    Transcribed,
    Tokenizing,
    Tokenized,
    Analyzing,
    Complete,
    Failed,
}

impl std::fmt::Display for JobStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStage::Queued => write!(f, "queued"),
            JobStage::Downloading => write!(f, "downloading"),
            JobStage::Downloaded => write!(f, "downloaded"),
            JobStage::Transcribing => write!(f, "transcribing"),
            JobStage::Transcribed => write!(f, "transcribed"),
            JobStage::Tokenizing => write!(f, "tokenizing"),
            JobStage::Tokenized => write!(f, "tokenized"),
            JobStage::Analyzing => write!(f, "analyzing"),
            JobStage::Complete => write!(f, "complete"),
            JobStage::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for JobStage {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(JobStage::Queued),
            "downloading" => Ok(JobStage::Downloading),
            "downloaded" => Ok(JobStage::Downloaded),
            "transcribing" => Ok(JobStage::Transcribing),
            "transcribed" => Ok(JobStage::Transcribed),
            "tokenizing" => Ok(JobStage::Tokenizing),
            "tokenized" => Ok(JobStage::Tokenized),
            "analyzing" => Ok(JobStage::Analyzing),
            "complete" => Ok(JobStage::Complete),
            "failed" => Ok(JobStage::Failed),
            _ => Err(anyhow::anyhow!("Invalid job stage: {}", s)),
        }
    }
}

/// Job representing a single episode to process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub anime_id: i64,
    pub anime_title: String,
    pub anime_title_english: Option<String>,
    pub mal_id: u32,
    pub episode: u32,
    pub season: Option<i32>,
    pub year: Option<i32>,

    // Job status
    pub stage: JobStage,
    pub progress: f64,  // 0.0 to 1.0

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    // Error handling
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,

    // File paths (relative to data directory)
    pub video_path: Option<String>,
    pub transcript_path: Option<String>,
    pub tokens_path: Option<String>,
    pub analysis_path: Option<String>,

    // Metadata (file sizes preserved for statistics)
    pub duration_seconds: Option<u32>,
    pub video_size_bytes: Option<u64>,
    pub audio_size_bytes: Option<u64>,
    pub transcript_size_bytes: Option<u64>,
    pub tokens_size_bytes: Option<u64>,

    // Word/token counts
    pub word_count: Option<u32>,
    pub token_count: Option<u32>,

    // Cleanup tracking
    pub video_deleted: bool,
    pub audio_deleted: bool,

    // Priority
    pub priority: i32,
    pub depends_on: Option<i64>,
}

/// New job to be created
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub anime_id: i64,
    pub mal_id: u32,
    pub anime_title: String,
    pub episode: u32,
    pub priority: i32,
}

/// File type for cleanup tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Video,
    Audio,
}

/// Job metadata update
#[derive(Debug, Clone, Default)]
pub struct JobMetadata {
    pub video_size_bytes: Option<u64>,
    pub audio_size_bytes: Option<u64>,
    pub transcript_size_bytes: Option<u64>,
    pub tokens_size_bytes: Option<u64>,
    pub duration_seconds: Option<u32>,
    pub word_count: Option<u32>,
    pub token_count: Option<u32>,
    pub video_path: Option<String>,
    pub transcript_path: Option<String>,
    pub tokens_path: Option<String>,
}

/// Anime selection result (cached from Claude Haiku)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeSelection {
    pub selected_index: i32,      // 1-based index from candidates list
    pub selected_title: String,   // The title that was selected
    pub confidence: String,        // "high", "medium", or "low"
    pub reason: String,            // Reason for selection
}
