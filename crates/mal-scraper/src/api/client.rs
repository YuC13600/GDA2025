//! Jikan API client with rate limiting and retry logic.

use super::rate_limiter::RateLimiter;
use super::types::*;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, StatusCode};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Jikan API v4 client
pub struct JikanClient {
    /// HTTP client
    client: Client,
    /// Base URL for Jikan API
    base_url: String,
    /// Rate limiter
    rate_limiter: RateLimiter,
    /// Maximum retries for failed requests
    max_retries: u32,
    /// Base delay for retry (exponential backoff)
    retry_delay_ms: u64,
}

impl JikanClient {
    /// Create a new Jikan client
    pub fn new(
        base_url: String,
        requests_per_second: f64,
        requests_per_minute: u32,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("GDA2025-Zipf-Analysis/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url,
            rate_limiter: RateLimiter::new(requests_per_second, requests_per_minute),
            max_retries,
            retry_delay_ms,
        })
    }

    /// Make a GET request with rate limiting and retry logic
    async fn get<T: serde::de::DeserializeOwned>(&mut self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);

        for attempt in 0..=self.max_retries {
            // Apply rate limiting before each request
            self.rate_limiter.acquire().await;

            debug!(url = %url, attempt = attempt + 1, "Making API request");

            match self.client.get(&url).send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        // Parse response
                        match response.json::<T>().await {
                            Ok(data) => {
                                debug!(url = %url, "Request successful");
                                return Ok(data);
                            }
                            Err(e) => {
                                warn!(url = %url, error = %e, "Failed to parse response");
                                return Err(anyhow!("Failed to parse response: {}", e));
                            }
                        }
                    } else if status == StatusCode::TOO_MANY_REQUESTS {
                        // Rate limited by server - wait longer
                        let delay = Duration::from_millis(self.retry_delay_ms * 2u64.pow(attempt));
                        warn!(
                            url = %url,
                            delay_ms = delay.as_millis(),
                            "Rate limited by server, waiting"
                        );
                        sleep(delay).await;
                        continue;
                    } else {
                        // Try to parse error response
                        let error_text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        warn!(
                            url = %url,
                            status = %status,
                            error = %error_text,
                            "Request failed"
                        );

                        if attempt < self.max_retries {
                            let delay = Duration::from_millis(self.retry_delay_ms * 2u64.pow(attempt));
                            debug!(delay_ms = delay.as_millis(), "Retrying after delay");
                            sleep(delay).await;
                            continue;
                        } else {
                            return Err(anyhow!(
                                "Request failed with status {}: {}",
                                status,
                                error_text
                            ));
                        }
                    }
                }
                Err(e) => {
                    warn!(url = %url, error = %e, "Request error");

                    if attempt < self.max_retries {
                        let delay = Duration::from_millis(self.retry_delay_ms * 2u64.pow(attempt));
                        debug!(delay_ms = delay.as_millis(), "Retrying after delay");
                        sleep(delay).await;
                        continue;
                    } else {
                        return Err(anyhow!("Request failed after {} retries: {}", self.max_retries, e));
                    }
                }
            }
        }

        Err(anyhow!("Request failed after all retries"))
    }

    /// Fetch all genres
    pub async fn get_genres(&mut self) -> Result<Vec<CategoryItem>> {
        info!("Fetching anime genres");
        let response: DataResponse<CategoryItem> = self.get("/genres/anime").await?;
        Ok(response.data)
    }

    /// Fetch all explicit genres
    pub async fn get_explicit_genres(&mut self) -> Result<Vec<CategoryItem>> {
        info!("Fetching explicit genres");
        let response: DataResponse<CategoryItem> = self.get("/genres/anime?filter=explicit_genres").await?;
        Ok(response.data)
    }

    /// Fetch all themes
    pub async fn get_themes(&mut self) -> Result<Vec<CategoryItem>> {
        info!("Fetching anime themes");
        let response: DataResponse<CategoryItem> = self.get("/genres/anime?filter=themes").await?;
        Ok(response.data)
    }

    /// Fetch all demographics
    pub async fn get_demographics(&mut self) -> Result<Vec<CategoryItem>> {
        info!("Fetching demographics");
        let response: DataResponse<CategoryItem> = self.get("/genres/anime?filter=demographics").await?;
        Ok(response.data)
    }

    /// Fetch producers/studios (paginated)
    pub async fn get_producers(&mut self, page: u32) -> Result<PaginatedResponse<ProducerItem>> {
        info!(page = page, "Fetching producers/studios");
        self.get(&format!("/producers?page={}", page)).await
    }

    /// Fetch top anime for a specific genre
    pub async fn get_top_anime_by_genre(&mut self, genre_id: u32, page: u32) -> Result<TopAnimeResponse> {
        info!(genre_id = genre_id, page = page, "Fetching top anime by genre");
        self.get(&format!("/top/anime?filter=bypopularity&genre={}&page={}", genre_id, page)).await
    }

    /// Fetch top anime for a specific producer/studio
    pub async fn get_top_anime_by_producer(&mut self, producer_id: u32, page: u32) -> Result<PaginatedResponse<TopAnimeEntry>> {
        info!(producer_id = producer_id, page = page, "Fetching top anime by producer");
        self.get(&format!("/anime?producer={}&page={}&order_by=members&sort=desc", producer_id, page)).await
    }

    /// Fetch full anime details by MAL ID
    pub async fn get_anime_details(&mut self, mal_id: u32) -> Result<AnimeDetails> {
        debug!(mal_id = mal_id, "Fetching anime details");
        let response: AnimeDetailsResponse = self.get(&format!("/anime/{}", mal_id)).await?;
        Ok(response.data)
    }

    /// Get current rate limit statistics
    pub fn rate_limit_stats(&mut self) -> (usize, u32) {
        let current_minute = self.rate_limiter.current_minute_count();
        let max_minute = 50; // From config
        (current_minute, max_minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = JikanClient::new(
            "https://api.jikan.moe/v4".to_string(),
            2.0,
            50,
            3,
            1000,
        );
        assert!(client.is_ok());
    }
}
