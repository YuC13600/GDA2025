# MAL Scraper - Final Review Checklist

**Date**: 2025-11-09
**Status**: ✅ Ready for Implementation

---

## ✅ Core Requirements

### Category Selection
- [x] Auto-discover ALL categories with ≥50 anime
- [x] Process: Genres, Explicit Genres, Themes, Demographics, Studios
- [x] Expected: ~130-200 categories
- [x] Expected unique anime: ~800-1200
- [x] Expected episodes: ~12,000-18,000
- [x] Configurable `min_category_size`
- [x] Configurable exclusions

### Ranking Strategy
- [x] Record ONLY global `rank` field
- [x] Do NOT create `anime_category_rankings` table
- [x] Purpose: Analyze ranking intervals (1-100, 101-500, etc.) vs Zipf fit
- [x] Database index on `rank` for efficient querying

### Deduplication
- [x] Use `HashSet<mal_id>` to collect unique anime
- [x] Database `UNIQUE(mal_id)` constraint
- [x] Database `UNIQUE(anime_id, episode)` constraint on jobs
- [x] Deduplication stats in output

---

## ✅ API Configuration

### Rate Limiting
- [x] Conservative: 2 req/s, 50 req/min
- [x] Max concurrent: 1 (no parallelism)
- [x] Jikan API limits documented: 3 req/s, 60 req/min
- [x] Timeout: 30s request, 10s connect

### Retry Strategy
- [x] Max retries: 5
- [x] Exponential backoff: 1s → 2s → 4s → 8s → 16s
- [x] Max delay: 60s
- [x] Retryable errors defined
- [x] Non-retryable errors defined

### Cache Strategy
- [x] Permanent cache (never expire)
- [x] Cache directory structure defined
- [x] JSON format (human-readable)
- [x] No compression
- [x] Auto-refresh on missing
- [x] Expected size: ~20 MB

---

## ✅ Database Design

### Anime Table
- [x] All required fields:
  - [x] mal_id (UNIQUE)
  - [x] Titles (main, English, Japanese, synonyms)
  - [x] Type, status, episodes
  - [x] Dates (aired_from, aired_to, season, year)
  - [x] Categories (genres, explicit_genres, themes, demographics, studios) as JSON arrays
  - [x] Scores (score, scored_by, rank, popularity)
  - [x] Additional (source, rating, duration_minutes)
  - [x] Processing status
  - [x] Timestamps
- [x] Indexes:
  - [x] mal_id
  - [x] rank (for interval analysis)
  - [x] score
  - [x] processing_status
- [x] Trigger for auto-updating `updated_at`

### Jobs Table (from TECHNICAL_DETAILS.md)
- [x] UNIQUE(anime_id, episode) constraint
- [x] File size tracking (video, audio, transcript, tokens)
- [x] Cleanup tracking flags
- [x] Foreign key to anime table

---

## ✅ File Size Tracking

### Where File Sizes are Recorded

**In Database**:
- [x] `jobs.video_size_bytes` - preserved even after deletion
- [x] `jobs.audio_size_bytes` - preserved even after deletion
- [x] `jobs.transcript_size_bytes`
- [x] `jobs.tokens_size_bytes`

**In Logs**:
- [x] Cache file writes logged with `size_bytes`
- [x] Summary statistics logged at completion

**In Statistics JSON**:
- [x] `file_sizes.category_cache_bytes`
- [x] `file_sizes.anime_metadata_cache_bytes`
- [x] `file_sizes.total_cache_bytes`

**Purpose**: Track total data processed for publication statistics

---

## ✅ Logging Design

### Framework
- [x] Using `tracing` + `tracing-subscriber`
- [x] Structured logging with contextual fields

### Log Levels
- [x] ERROR: Failures, unrecoverable errors
- [x] WARN: Skipped categories, retries
- [x] INFO: Milestones, file I/O with sizes
- [x] DEBUG: API requests, cache operations
- [x] TRACE: Detailed metadata, all requests

### Log Outputs
- [x] stdout (pretty format)
- [x] File (data/logs/mal_scraper.log)
- [x] Log rotation: daily
- [x] Max size: 100 MB
- [x] Keep 7 files

### File Size Logging
- [x] Every cache write logs `size_bytes`
- [x] Summary statistics at completion
- [x] Total cache size logged

### Module-specific Levels
- [x] mal_scraper::api = debug
- [x] mal_scraper::cache = info
- [x] reqwest = warn
- [x] rusqlite = warn

---

## ✅ Configuration

### config.toml Complete
- [x] [mal_scraper] - category selection
- [x] [mal_scraper.api] - rate limiting
- [x] [mal_scraper.retry] - retry strategy
- [x] [mal_scraper.cache] - cache config
- [x] [paths] - data directory, database
- [x] [logging] - complete logging config with rotation

### CLI Options
- [x] --config (config file path)
- [x] --data-dir
- [x] --dry-run
- [x] --clear-cache
- [x] --discover-only
- [x] --verbose
- [x] --stats-output
- [x] --min-category-size (override)
- [x] --exclude (override)

---

## ✅ Output Files

### Cache Files
- [x] Structure: data/cache/mal_cache/
- [x] Categories by type (genres/, themes/, demographics/, studios/)
- [x] Anime metadata (anime/)
- [x] Size: ~20 MB total

### Statistics JSON
- [x] Scrape date and version
- [x] Config used
- [x] Categories processed/skipped
- [x] Deduplication stats
- [x] Jobs created
- [x] Ranking distribution
- [x] **File sizes** ⭐
- [x] Performance metrics

### Database
- [x] jobs.db with populated anime and jobs tables
- [x] File size fields preserved

### Logs
- [x] data/logs/mal_scraper.log
- [x] Rotated daily, keep 7 files
- [x] File sizes logged

---

## ✅ Error Handling

### Error Types
- [x] ApiError
- [x] RateLimitExceeded
- [x] NetworkError
- [x] CacheError
- [x] DatabaseError
- [x] SerializationError
- [x] ConfigError

### Error Context
- [x] All errors log with contextual fields
- [x] Retry count logged
- [x] mal_id/title logged where applicable

---

## ✅ Implementation Plan

### Phases Defined
- [x] Phase 1: Core Infrastructure
- [x] Phase 2: API Client
- [x] Phase 3: Cache System
- [x] Phase 4: Category Discovery
- [x] Phase 5: Anime Fetching
- [x] Phase 6: Job Creation
- [x] Phase 7: CLI & Stats
- [x] Phase 8: Testing

### Dependencies
- [x] reqwest (HTTP client)
- [x] jikan-rs (Jikan API wrapper)
- [x] serde, serde_json (serialization)
- [x] rusqlite (database)
- [x] tracing, tracing-subscriber (logging)
- [x] clap (CLI)
- [x] tokio (async runtime)
- [x] anyhow, thiserror (error handling)

### Timeline
- [x] Estimated: 12-18 days development
- [x] Runtime: ~11 minutes first run, <1 min with cache

---

## ✅ Testing Strategy

### Unit Tests
- [x] API client with mocked responses
- [x] Cache read/write operations
- [x] Deduplication logic
- [x] Rate limiter

### Integration Tests
- [x] Small test dataset (2-3 categories)
- [x] End-to-end flow
- [x] Cache invalidation
- [x] Error handling

### Manual Testing
- [x] Dry-run mode
- [x] Clear-cache and re-run
- [x] Discover-only mode
- [x] Stats validation

---

## 📝 Undecided/Future Considerations

### None - All Critical Decisions Made ✅

**All requirements for MAL Scraper are finalized and documented.**

---

## ⚠️ Important Notes

1. **File Size Tracking**: All cache file writes MUST log `size_bytes` for publication statistics
2. **Permanent Cache**: Cache never expires - only deleted manually or with --clear-cache
3. **Ranking**: Only global rank stored, NOT per-category rankings
4. **Conservative Rate Limiting**: 2 req/s to be safe with Jikan API
5. **Structured Logging**: Use `tracing` fields for all contextual data

---

## 🚀 Ready to Implement?

**Status**: ✅ **APPROVED - Ready for Implementation**

**Next Steps**:
1. Update CLAUDE.md with project structure and conventions
2. Create Cargo workspace
3. Implement Phase 1 (Core Infrastructure)
4. Begin development following MAL_SCRAPER_SPEC.md

---

**Reviewed by**: User
**Approved**: 2025-11-09
**Specification**: MAL_SCRAPER_SPEC.md
