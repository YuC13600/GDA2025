//! Category discovery and anime fetching logic.
//!
//! Auto-discovers all categories (genres, themes, demographics, studios) with
//! at least min_items entries, then fetches anime from each category.

use crate::api::JikanClient;
use crate::cache::CacheManager;
use anyhow::Result;
use chrono::Utc;
use shared::{Anime, ProcessingStatus};
use std::collections::HashSet;
use tracing::{info, warn};

/// Category type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryType {
    Genre,
    ExplicitGenre,
    Theme,
    Demographic,
    Studio,
}

impl CategoryType {
    pub fn as_str(&self) -> &str {
        match self {
            CategoryType::Genre => "genre",
            CategoryType::ExplicitGenre => "explicit_genre",
            CategoryType::Theme => "theme",
            CategoryType::Demographic => "demographic",
            CategoryType::Studio => "studio",
        }
    }
}

/// Category with metadata
#[derive(Debug, Clone)]
pub struct Category {
    pub category_type: CategoryType,
    pub mal_id: u32,
    pub name: String,
    pub count: u32,
}

/// Discovery manager for finding categories and anime
pub struct DiscoveryManager {
    client: JikanClient,
    cache: CacheManager,
    min_category_items: usize,
}

impl DiscoveryManager {
    /// Create a new discovery manager
    pub fn new(
        client: JikanClient,
        cache: CacheManager,
        min_category_items: usize,
    ) -> Self {
        Self {
            client,
            cache,
            min_category_items,
        }
    }

    /// Discover all categories that meet the minimum item threshold
    pub async fn discover_categories(&mut self) -> Result<Vec<Category>> {
        info!(
            min_items = self.min_category_items,
            "Starting category discovery"
        );

        let mut categories = Vec::new();

        // Fetch genres
        info!("Discovering genres");
        let cache_key = "genres";
        let genres = if let Some(cached) = self.cache.get(cache_key)? {
            cached
        } else {
            let data = self.client.get_genres().await?;
            self.cache.set(cache_key, &data)?;
            data
        };

        categories.extend(
            genres
                .into_iter()
                .filter(|g| g.count >= self.min_category_items as u32)
                .map(|g| Category {
                    category_type: CategoryType::Genre,
                    mal_id: g.mal_id,
                    name: g.name,
                    count: g.count,
                }),
        );

        // Fetch explicit genres
        info!("Discovering explicit genres");
        let cache_key = "explicit_genres";
        let explicit_genres = if let Some(cached) = self.cache.get(cache_key)? {
            cached
        } else {
            let data = self.client.get_explicit_genres().await?;
            self.cache.set(cache_key, &data)?;
            data
        };

        categories.extend(
            explicit_genres
                .into_iter()
                .filter(|g| g.count >= self.min_category_items as u32)
                .map(|g| Category {
                    category_type: CategoryType::ExplicitGenre,
                    mal_id: g.mal_id,
                    name: g.name,
                    count: g.count,
                }),
        );

        // Fetch themes
        info!("Discovering themes");
        let cache_key = "themes";
        let themes = if let Some(cached) = self.cache.get(cache_key)? {
            cached
        } else {
            let data = self.client.get_themes().await?;
            self.cache.set(cache_key, &data)?;
            data
        };

        categories.extend(
            themes
                .into_iter()
                .filter(|g| g.count >= self.min_category_items as u32)
                .map(|g| Category {
                    category_type: CategoryType::Theme,
                    mal_id: g.mal_id,
                    name: g.name,
                    count: g.count,
                }),
        );

        // Fetch demographics
        info!("Discovering demographics");
        let cache_key = "demographics";
        let demographics = if let Some(cached) = self.cache.get(cache_key)? {
            cached
        } else {
            let data = self.client.get_demographics().await?;
            self.cache.set(cache_key, &data)?;
            data
        };

        categories.extend(
            demographics
                .into_iter()
                .filter(|g| g.count >= self.min_category_items as u32)
                .map(|g| Category {
                    category_type: CategoryType::Demographic,
                    mal_id: g.mal_id,
                    name: g.name,
                    count: g.count,
                }),
        );

        // Fetch studios (paginated)
        info!("Discovering studios");
        let mut studios_count = 0;
        let mut page = 1;
        loop {
            let cache_key = format!("studios_page_{}", page);
            let response = if let Some(cached) = self.cache.get(&cache_key)? {
                cached
            } else {
                let data = self.client.get_producers(page).await?;
                self.cache.set(&cache_key, &data)?;
                data
            };

            let filtered: Vec<_> = response
                .data
                .into_iter()
                .filter(|p| p.count >= self.min_category_items as u32)
                .collect();

            studios_count += filtered.len();
            categories.extend(filtered.into_iter().map(|p| {
                // Get the Default title, or first available title
                let name = p.titles
                    .iter()
                    .find(|t| t.title_type == "Default")
                    .or_else(|| p.titles.first())
                    .map(|t| t.title.clone())
                    .unwrap_or_else(|| format!("Studio {}", p.mal_id));

                Category {
                    category_type: CategoryType::Studio,
                    mal_id: p.mal_id,
                    name,
                    count: p.count,
                }
            }));

            if !response.pagination.has_next_page {
                break;
            }
            page += 1;
        }

        info!(
            total_categories = categories.len(),
            genres = categories.iter().filter(|c| c.category_type == CategoryType::Genre).count(),
            explicit_genres = categories.iter().filter(|c| c.category_type == CategoryType::ExplicitGenre).count(),
            themes = categories.iter().filter(|c| c.category_type == CategoryType::Theme).count(),
            demographics = categories.iter().filter(|c| c.category_type == CategoryType::Demographic).count(),
            studios = studios_count,
            "Category discovery complete"
        );

        Ok(categories)
    }

    /// Fetch anime IDs for a specific category
    pub async fn fetch_anime_ids_for_category(
        &mut self,
        category: &Category,
    ) -> Result<Vec<u32>> {
        info!(
            category_type = category.category_type.as_str(),
            category_name = %category.name,
            category_id = category.mal_id,
            "Fetching anime IDs for category"
        );

        let mut anime_ids = HashSet::new();

        match category.category_type {
            CategoryType::Studio => {
                // Fetch by producer
                let mut page = 1;
                loop {
                    let cache_key = format!(
                        "anime_studio_{}_page_{}",
                        category.mal_id,
                        page
                    );

                    let response = if let Some(cached) = self.cache.get(&cache_key)? {
                        cached
                    } else {
                        let data = self.client.get_top_anime_by_producer(category.mal_id, page).await?;
                        self.cache.set(&cache_key, &data)?;
                        data
                    };

                    for anime in &response.data {
                        anime_ids.insert(anime.mal_id);
                    }

                    if !response.pagination.has_next_page {
                        break;
                    }
                    page += 1;

                    // Limit to reasonable number of pages
                    if page > 10 {
                        warn!(
                            category = %category.name,
                            "Reached page limit for category"
                        );
                        break;
                    }
                }
            }
            _ => {
                // Fetch by genre/theme/demographic
                let mut page = 1;
                loop {
                    let cache_key = format!(
                        "anime_{}_{}_page_{}",
                        category.category_type.as_str(),
                        category.mal_id,
                        page
                    );

                    let response = if let Some(cached) = self.cache.get(&cache_key)? {
                        cached
                    } else {
                        let data = self.client.get_top_anime_by_genre(category.mal_id, page).await?;
                        self.cache.set(&cache_key, &data)?;
                        data
                    };

                    for anime in &response.data {
                        anime_ids.insert(anime.mal_id);
                    }

                    // Top anime endpoint doesn't have reliable pagination
                    if response.data.is_empty() {
                        break;
                    }
                    page += 1;

                    // Limit to reasonable number of pages
                    if page > 10 {
                        warn!(
                            category = %category.name,
                            "Reached page limit for category"
                        );
                        break;
                    }
                }
            }
        }

        info!(
            category_name = %category.name,
            anime_count = anime_ids.len(),
            "Fetched anime IDs for category"
        );

        Ok(anime_ids.into_iter().collect())
    }

    /// Fetch full anime details by MAL ID
    pub async fn fetch_anime_details(&mut self, mal_id: u32) -> Result<Anime> {
        let cache_key = format!("anime_{}", mal_id);

        let details = if let Some(cached) = self.cache.get(&cache_key)? {
            cached
        } else {
            let data = self.client.get_anime_details(mal_id).await?;
            self.cache.set(&cache_key, &data)?;
            data
        };

        // Convert aired dates
        let aired_from = details.aired.from.as_ref().and_then(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
        });
        let aired_to = details.aired.to.as_ref().and_then(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
        });

        // Convert to our Anime model
        let anime = Anime {
            id: None,
            mal_id: details.mal_id,
            title: details.title,
            title_english: details.title_english,
            title_japanese: details.title_japanese,
            title_synonyms: details.title_synonyms,
            anime_type: details.anime_type,
            episodes_total: details.episodes,
            status: details.status,
            aired_from,
            aired_to,
            season: details.season,
            year: details.year.map(|y| y as i32),
            genres: details.genres.iter().map(|g| g.name.clone()).collect(),
            explicit_genres: details.explicit_genres.iter().map(|g| g.name.clone()).collect(),
            themes: details.themes.iter().map(|t| t.name.clone()).collect(),
            demographics: details.demographics.iter().map(|d| d.name.clone()).collect(),
            studios: details.studios.iter().map(|s| s.name.clone()).collect(),
            score: details.score,
            scored_by: details.scored_by,
            rank: details.rank,
            popularity: details.popularity,
            source: details.source,
            rating: details.rating,
            duration_minutes: details.duration.as_ref().and_then(|d| {
                // Parse duration string like "24 min per ep" to minutes
                d.split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
            }),
            episodes_processed: 0,
            processing_status: ProcessingStatus::Pending,
            fetched_at: Utc::now(),
            updated_at: Utc::now(),
        };

        Ok(anime)
    }
}
