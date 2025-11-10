//! Main scraper orchestrator.
//!
//! Coordinates the entire MAL scraping process: discover categories,
//! fetch anime, and save to database.

use crate::discovery::DiscoveryManager;
use anyhow::{Context, Result};
use shared::{JobQueue, NewJob};
use std::collections::HashSet;
use tracing::{error, info, warn};

/// Statistics for scraping session
#[derive(Debug, Clone, Default)]
pub struct ScraperStats {
    pub total_categories: usize,
    pub total_anime_discovered: usize,
    pub unique_anime: usize,
    pub anime_saved: usize,
    pub jobs_created: usize,
    pub errors: usize,
}

/// Main scraper coordinator
pub struct MalScraper {
    discovery: DiscoveryManager,
    job_queue: JobQueue,
}

impl MalScraper {
    /// Create a new MAL scraper
    pub fn new(discovery: DiscoveryManager, job_queue: JobQueue) -> Self {
        Self {
            discovery,
            job_queue,
        }
    }

    /// Run the complete scraping process
    ///
    /// This is the main entry point that orchestrates:
    /// 1. Category discovery
    /// 2. Anime fetching (streaming, not accumulating in memory)
    /// 3. Database storage
    /// 4. Job creation
    pub async fn run(&mut self) -> Result<ScraperStats> {
        info!("Starting MAL scraper");

        let mut stats = ScraperStats::default();

        // Phase 1: Discover all categories
        info!("Phase 1: Discovering categories");
        let categories = self
            .discovery
            .discover_categories()
            .await
            .context("Failed to discover categories")?;

        stats.total_categories = categories.len();
        info!(
            categories = stats.total_categories,
            "Discovered categories"
        );

        // Track unique anime across all categories
        let mut all_anime_ids = HashSet::new();

        // Phase 2: Fetch anime IDs for each category (streaming)
        info!("Phase 2: Fetching anime IDs for categories");
        for (idx, category) in categories.iter().enumerate() {
            info!(
                progress = format!("{}/{}", idx + 1, categories.len()),
                category = %category.name,
                category_type = category.category_type.as_str(),
                "Processing category"
            );

            match self
                .discovery
                .fetch_anime_ids_for_category(category)
                .await
            {
                Ok(anime_ids) => {
                    stats.total_anime_discovered += anime_ids.len();
                    for id in anime_ids {
                        all_anime_ids.insert(id);
                    }
                }
                Err(e) => {
                    error!(
                        category = %category.name,
                        error = %e,
                        "Failed to fetch anime for category"
                    );
                    stats.errors += 1;
                }
            }
        }

        stats.unique_anime = all_anime_ids.len();
        info!(
            total_discovered = stats.total_anime_discovered,
            unique = stats.unique_anime,
            "Discovered anime across all categories"
        );

        // Phase 3: Fetch anime details and save to database (streaming)
        info!("Phase 3: Fetching anime details and saving to database");
        let anime_vec: Vec<u32> = all_anime_ids.into_iter().collect();

        for (idx, mal_id) in anime_vec.iter().enumerate() {
            if (idx + 1) % 100 == 0 || idx + 1 == anime_vec.len() {
                info!(
                    progress = format!("{}/{}", idx + 1, anime_vec.len()),
                    "Fetching anime details"
                );
            }

            match self.fetch_and_save_anime(*mal_id).await {
                Ok(jobs_created) => {
                    stats.anime_saved += 1;
                    stats.jobs_created += jobs_created;
                }
                Err(e) => {
                    error!(mal_id = mal_id, error = %e, "Failed to fetch anime");
                    stats.errors += 1;
                }
            }
        }

        info!(
            categories = stats.total_categories,
            total_anime_discovered = stats.total_anime_discovered,
            unique_anime = stats.unique_anime,
            anime_saved = stats.anime_saved,
            jobs_created = stats.jobs_created,
            errors = stats.errors,
            "MAL scraper complete"
        );

        Ok(stats)
    }

    /// Fetch anime details and save to database (with deduplication)
    ///
    /// Returns the number of jobs created
    async fn fetch_and_save_anime(&mut self, mal_id: u32) -> Result<usize> {
        // Fetch anime details from API (cached)
        let anime = self
            .discovery
            .fetch_anime_details(mal_id)
            .await
            .with_context(|| format!("Failed to fetch anime {}", mal_id))?;

        // Save to database (with deduplication)
        let anime_id = self
            .job_queue
            .get_or_create_anime(&anime)
            .context("Failed to save anime to database")?;

        // Create jobs for each episode
        let episodes = anime.episodes_total.unwrap_or(0);

        if episodes == 0 {
            warn!(
                mal_id = mal_id,
                title = %anime.title,
                "Anime has 0 episodes, skipping job creation"
            );
            return Ok(0);
        }

        let mut jobs_created = 0;
        for episode in 1..=episodes {
            let new_job = NewJob {
                anime_id,
                mal_id: anime.mal_id,
                anime_title: anime.title.clone(),
                episode,
                priority: 0, // Default priority
            };

            match self.job_queue.enqueue(&new_job) {
                Ok(_) => jobs_created += 1,
                Err(e) => {
                    // Log but don't fail - job might already exist
                    warn!(
                        anime_id = anime_id,
                        episode = episode,
                        error = %e,
                        "Failed to create job (might already exist)"
                    );
                }
            }
        }

        Ok(jobs_created)
    }

    /// Get current scraping statistics
    pub fn get_queue_stats(&self) -> Result<shared::queue::JobStats> {
        self.job_queue.get_stats()
    }
}
