//! Jikan API v4 response types.
//!
//! These types represent the JSON responses from the Jikan API.

use serde::{Deserialize, Serialize};

/// Generic pagination wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}

/// Simple data wrapper (without pagination)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataResponse<T> {
    pub data: Vec<T>,
}

/// Pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub last_visible_page: u32,
    pub has_next_page: bool,
    pub current_page: u32,
    #[serde(default)]
    pub items: Option<PaginationItems>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationItems {
    pub count: u32,
    pub total: u32,
    pub per_page: u32,
}

/// Genre/Theme/Demographic item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryItem {
    pub mal_id: u32,
    pub name: String,
    pub url: String,
    pub count: u32,
}

/// Producer/Studio item (different structure from CategoryItem)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerItem {
    pub mal_id: u32,
    pub titles: Vec<ProducerTitle>,
    pub url: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerTitle {
    #[serde(rename = "type")]
    pub title_type: String,
    pub title: String,
}

/// Top anime response (used for category top lists)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopAnimeResponse {
    pub data: Vec<TopAnimeEntry>,
}

/// Top anime entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopAnimeEntry {
    pub mal_id: u32,
    pub url: String,
    pub images: AnimeImages,
    pub title: String,
    pub title_english: Option<String>,
    pub title_japanese: Option<String>,
    #[serde(rename = "type")]
    pub anime_type: Option<String>,
    pub episodes: Option<u32>,
    pub status: Option<String>,
    pub score: Option<f64>,
    pub scored_by: Option<u32>,
    pub rank: Option<u32>,
    pub popularity: Option<u32>,
    pub members: Option<u32>,
    pub favorites: Option<u32>,
}

/// Full anime details response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeDetailsResponse {
    pub data: AnimeDetails,
}

/// Full anime details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeDetails {
    pub mal_id: u32,
    pub url: String,
    pub images: AnimeImages,

    // Titles
    pub title: String,
    pub title_english: Option<String>,
    pub title_japanese: Option<String>,
    pub title_synonyms: Vec<String>,

    // Type and status
    #[serde(rename = "type")]
    pub anime_type: Option<String>,
    pub source: Option<String>,
    pub episodes: Option<u32>,
    pub status: Option<String>,
    pub airing: bool,

    // Dates
    pub aired: Aired,
    pub duration: Option<String>,
    pub rating: Option<String>,

    // Scores and rankings
    pub score: Option<f64>,
    pub scored_by: Option<u32>,
    pub rank: Option<u32>,
    pub popularity: Option<u32>,
    pub members: Option<u32>,
    pub favorites: Option<u32>,

    // Synopsis
    pub synopsis: Option<String>,
    pub background: Option<String>,

    // Season
    pub season: Option<String>,
    pub year: Option<u32>,

    // Broadcast
    pub broadcast: Option<Broadcast>,

    // Producers, licensors, studios
    pub producers: Vec<MalEntity>,
    pub licensors: Vec<MalEntity>,
    pub studios: Vec<MalEntity>,

    // Genres, themes, demographics
    pub genres: Vec<MalEntity>,
    pub explicit_genres: Vec<MalEntity>,
    pub themes: Vec<MalEntity>,
    pub demographics: Vec<MalEntity>,
}

/// Anime images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimeImages {
    pub jpg: ImageSet,
    #[serde(default)]
    pub webp: Option<ImageSet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSet {
    pub image_url: Option<String>,
    pub small_image_url: Option<String>,
    pub large_image_url: Option<String>,
}

/// Aired dates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aired {
    pub from: Option<String>,
    pub to: Option<String>,
    pub prop: AiredProp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiredProp {
    pub from: DateProp,
    pub to: DateProp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateProp {
    pub day: Option<u32>,
    pub month: Option<u32>,
    pub year: Option<u32>,
}

/// Broadcast information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Broadcast {
    pub day: Option<String>,
    pub time: Option<String>,
    pub timezone: Option<String>,
    pub string: Option<String>,
}

/// MAL entity (genre, studio, producer, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalEntity {
    pub mal_id: u32,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    pub url: String,
}

/// Error response from Jikan API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JikanError {
    pub status: u16,
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
}
