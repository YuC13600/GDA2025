//! Jikan API v4 client implementation.
//!
//! This module provides a rate-limited, retry-enabled client for interacting
//! with the Jikan API (MyAnimeList unofficial API).

pub mod client;
pub mod rate_limiter;
pub mod types;

pub use client::JikanClient;
pub use rate_limiter::RateLimiter;
pub use types::*;
