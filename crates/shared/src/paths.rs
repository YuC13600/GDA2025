//! File path utilities for organizing data files.
//!
//! This module provides a centralized way to manage file paths for all data files
//! (videos, audio, transcripts, tokens, analysis results, cache, etc.).

use std::path::{Path, PathBuf};

/// File path manager for data files
#[derive(Debug, Clone)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    /// Create a new DataPaths with the given root directory
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the root data directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    // ========== Video paths (TEMPORARY - auto-deleted) ==========

    /// Get video directory for an anime
    pub fn video_dir(&self, anime_id: u32) -> PathBuf {
        self.root
            .join("videos")
            .join(anime_id.to_string())
            .join("episodes")
    }

    /// Get video file path for an episode
    pub fn video_file(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.video_dir(anime_id)
            .join(format!("ep{:03}.mkv", episode))
    }

    // ========== Audio paths (TEMPORARY - auto-deleted) ==========

    /// Get audio directory for an anime
    pub fn audio_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("audio").join(anime_id.to_string())
    }

    /// Get audio file path for an episode
    pub fn audio_file(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.audio_dir(anime_id)
            .join(format!("ep{:03}.wav", episode))
    }

    // ========== Transcript paths (PERMANENT) ==========

    /// Get transcript directory for an anime
    pub fn transcript_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("transcripts").join(anime_id.to_string())
    }

    /// Get plain text transcript path
    pub fn transcript_txt(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.transcript_dir(anime_id)
            .join(format!("ep{:03}.txt", episode))
    }

    /// Get JSON transcript path (with timestamps and metadata)
    pub fn transcript_json(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.transcript_dir(anime_id)
            .join(format!("ep{:03}.json", episode))
    }

    // ========== Token paths (PERMANENT) ==========

    /// Get tokens directory for an anime
    pub fn tokens_dir(&self, anime_id: u32) -> PathBuf {
        self.root.join("tokens").join(anime_id.to_string())
    }

    /// Get full tokenization JSON path
    pub fn tokens_json(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.tokens_dir(anime_id)
            .join(format!("ep{:03}_tokens.json", episode))
    }

    /// Get word frequency CSV path
    pub fn freq_csv(&self, anime_id: u32, episode: u32) -> PathBuf {
        self.tokens_dir(anime_id)
            .join(format!("ep{:03}_freq.csv", episode))
    }

    // ========== Analysis paths (PERMANENT) ==========

    /// Get analysis directory for an anime
    pub fn analysis_dir(&self, anime_id: u32) -> PathBuf {
        self.root
            .join("analysis")
            .join("per_anime")
            .join(anime_id.to_string())
    }

    /// Get Zipf parameters JSON path
    pub fn zipf_params(&self, anime_id: u32) -> PathBuf {
        self.analysis_dir(anime_id).join("zipf_params.json")
    }

    /// Get Zipf plot HTML path
    pub fn zipf_plot(&self, anime_id: u32) -> PathBuf {
        self.analysis_dir(anime_id).join("zipf_plot.html")
    }

    /// Get statistics summary JSON path
    pub fn statistics(&self, anime_id: u32) -> PathBuf {
        self.analysis_dir(anime_id).join("statistics.json")
    }

    // ========== Metadata ==========

    /// Get anime metadata JSON path
    pub fn anime_metadata(&self, anime_id: u32) -> PathBuf {
        self.root
            .join("videos")
            .join(anime_id.to_string())
            .join("metadata.json")
    }

    // ========== Cache ==========

    /// Get cache directory
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    /// Get MAL cache directory
    pub fn mal_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("mal_cache")
    }

    /// Get category cache directory
    pub fn category_cache_dir(&self, category_type: &str) -> PathBuf {
        self.mal_cache_dir()
            .join("categories")
            .join(category_type)
    }

    /// Get anime cache directory
    pub fn anime_cache_dir(&self) -> PathBuf {
        self.mal_cache_dir().join("anime")
    }

    /// Get cached category file
    pub fn category_cache_file(&self, category_type: &str, category_name: &str) -> PathBuf {
        self.category_cache_dir(category_type)
            .join(format!("{}_top50.json", category_name.to_lowercase()))
    }

    /// Get cached anime metadata file
    pub fn anime_cache_file(&self, mal_id: u32, title_slug: &str) -> PathBuf {
        self.anime_cache_dir()
            .join(format!("{}_{}.json", mal_id, title_slug))
    }

    // ========== Database ==========

    /// Get database path
    pub fn jobs_db(&self) -> PathBuf {
        self.root.join("jobs.db")
    }

    // ========== Logs ==========

    /// Get logs directory
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Get log file path for a specific component
    pub fn log_file(&self, component: &str) -> PathBuf {
        self.logs_dir().join(format!("{}.log", component))
    }

    // ========== Models ==========

    /// Get models directory (for Whisper models)
    pub fn models_dir(&self) -> PathBuf {
        self.root.parent().unwrap_or(&self.root).join("models")
    }

    /// Get Whisper model file path
    pub fn whisper_model(&self, model_name: &str) -> PathBuf {
        self.models_dir().join(format!("ggml-{}.bin", model_name))
    }

    // ========== Aggregated Analysis ==========

    /// Get aggregated analysis directory
    pub fn aggregated_dir(&self) -> PathBuf {
        self.root.join("analysis").join("aggregated")
    }

    /// Get genre-specific analysis directory
    pub fn genre_analysis_dir(&self, genre: &str) -> PathBuf {
        self.aggregated_dir()
            .join("by_genre")
            .join(genre.to_lowercase())
    }

    /// Get studio-specific analysis directory
    pub fn studio_analysis_dir(&self, studio: &str) -> PathBuf {
        self.aggregated_dir()
            .join("by_studio")
            .join(studio.to_lowercase())
    }

    // ========== Utility Methods ==========

    /// Create all necessary directories
    pub fn create_dirs(&self) -> std::io::Result<()> {
        let dirs = vec![
            self.root.join("videos"),
            self.root.join("audio"),
            self.root.join("transcripts"),
            self.root.join("tokens"),
            self.root.join("analysis/per_anime"),
            self.root.join("analysis/aggregated/by_genre"),
            self.root.join("analysis/aggregated/by_studio"),
            self.root.join("cache/mal_cache/categories/genres"),
            self.root.join("cache/mal_cache/categories/themes"),
            self.root.join("cache/mal_cache/categories/demographics"),
            self.root.join("cache/mal_cache/categories/studios"),
            self.root.join("cache/mal_cache/anime"),
            self.logs_dir(),
            self.models_dir(),
        ];

        for dir in dirs {
            std::fs::create_dir_all(&dir)?;
        }

        Ok(())
    }

    /// Create title slug from anime title (for cache filenames)
    pub fn title_to_slug(title: &str) -> String {
        title
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths() {
        let paths = DataPaths::new("/data");

        assert_eq!(
            paths.video_file(5114, 1),
            PathBuf::from("/data/videos/5114/episodes/ep001.mkv")
        );

        assert_eq!(
            paths.transcript_json(5114, 1),
            PathBuf::from("/data/transcripts/5114/ep001.json")
        );

        assert_eq!(
            paths.jobs_db(),
            PathBuf::from("/data/jobs.db")
        );
    }

    #[test]
    fn test_title_slug() {
        assert_eq!(
            DataPaths::title_to_slug("Fullmetal Alchemist: Brotherhood"),
            "fullmetal_alchemist_brotherhood"
        );

        assert_eq!(
            DataPaths::title_to_slug("鋼の錬金術師 FULLMETAL ALCHEMIST"),
            "fullmetal_alchemist"
        );
    }
}
