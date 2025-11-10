//! MAL Scraper library for fetching anime metadata from MyAnimeList.
//!
//! This library provides functionality to discover categories and fetch
//! anime information from the Jikan API v4.

pub mod api;
pub mod cache;
pub mod discovery;
pub mod scraper;

pub use api::{JikanClient, RateLimiter};
pub use cache::CacheManager;
pub use discovery::{Category, CategoryType, DiscoveryManager};
pub use scraper::{MalScraper, ScraperStats};
