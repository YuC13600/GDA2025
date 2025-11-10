//! Rate limiter implementation using token bucket algorithm.
//!
//! Enforces both per-second and per-minute rate limits for API requests.

use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Rate limiter with dual constraints (per-second and per-minute)
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum requests per second
    max_per_second: f64,
    /// Maximum requests per minute
    max_per_minute: u32,
    /// Last request timestamp
    last_request: Option<Instant>,
    /// Request timestamps in the last minute
    recent_requests: Vec<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(max_per_second: f64, max_per_minute: u32) -> Self {
        Self {
            max_per_second,
            max_per_minute,
            last_request: None,
            recent_requests: Vec::with_capacity(max_per_minute as usize),
        }
    }

    /// Wait until a request can be made, respecting both rate limits
    pub async fn acquire(&mut self) {
        let now = Instant::now();

        // Clean up requests older than 1 minute
        self.recent_requests
            .retain(|&timestamp| now.duration_since(timestamp) < Duration::from_secs(60));

        // Check per-minute limit
        if self.recent_requests.len() >= self.max_per_minute as usize {
            // Wait until the oldest request is more than 1 minute old
            if let Some(&oldest) = self.recent_requests.first() {
                let elapsed = now.duration_since(oldest);
                if elapsed < Duration::from_secs(60) {
                    let wait_time = Duration::from_secs(60) - elapsed;
                    tracing::debug!(
                        wait_ms = wait_time.as_millis(),
                        "Rate limit: waiting for per-minute limit"
                    );
                    sleep(wait_time).await;
                }
            }
        }

        // Check per-second limit
        if let Some(last) = self.last_request {
            let elapsed = now.duration_since(last);
            let min_interval = Duration::from_secs_f64(1.0 / self.max_per_second);

            if elapsed < min_interval {
                let wait_time = min_interval - elapsed;
                tracing::debug!(
                    wait_ms = wait_time.as_millis(),
                    "Rate limit: waiting for per-second limit"
                );
                sleep(wait_time).await;
            }
        }

        // Record this request
        let request_time = Instant::now();
        self.last_request = Some(request_time);
        self.recent_requests.push(request_time);
    }

    /// Get the current number of requests in the last minute
    pub fn current_minute_count(&mut self) -> usize {
        let now = Instant::now();
        self.recent_requests
            .retain(|&timestamp| now.duration_since(timestamp) < Duration::from_secs(60));
        self.recent_requests.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_per_second() {
        let mut limiter = RateLimiter::new(2.0, 50);

        let start = Instant::now();

        // Make 3 requests - should take at least 1 second
        for _ in 0..3 {
            limiter.acquire().await;
        }

        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(900)); // Allow some tolerance
    }

    #[tokio::test]
    async fn test_rate_limiter_per_minute() {
        let mut limiter = RateLimiter::new(100.0, 3); // High per-second, low per-minute

        let start = Instant::now();

        // Make 4 requests - should trigger per-minute limit
        for i in 0..4 {
            limiter.acquire().await;
            if i == 3 {
                // Fourth request should have waited
                let elapsed = start.elapsed();
                assert!(elapsed >= Duration::from_millis(50)); // Should have some delay
            }
        }
    }

    #[test]
    fn test_current_minute_count() {
        let mut limiter = RateLimiter::new(2.0, 50);
        assert_eq!(limiter.current_minute_count(), 0);
    }
}
