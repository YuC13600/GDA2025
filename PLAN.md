# Implementation Plan: Anime Zipf's Law Analysis Pipeline

## Project Overview

A modular Rust/Python hybrid system for analyzing Zipf's law in Japanese anime (scripted) vs livestreams (unscripted). The system consists of independent CLI tools coordinated through a job queue, with a separate TUI application for monitoring and scheduling.

## Technology Stack Summary

| Component | Language | Primary Libraries | Rationale |
|-----------|----------|-------------------|-----------|
| MAL Scraper | **Rust** | `jikan-rs`, `reqwest`, `scraper` | Jikan API wrapper available |
| Anime Downloader | **Rust wrapper** | `animdl` (via subprocess) | Best automation support |
| Speech-to-Text | **Rust** | `whisper-rs` | Native Rust bindings available |
| Tokenization | **Rust** | `vibrato` | Pure Rust, faster than MeCab CLI |
| Statistical Analysis | **Rust** | `polars`, `statrs`, `ndarray` | Excellent performance |
| Visualization | **Rust/Python** | `plotly` (Rust), matplotlib (fallback) | Hybrid approach |
| TUI Monitor | **Rust** | `ratatui`, `crossterm` | Modern TUI framework |
| Job Queue | **Rust** | SQLite via `rusqlite` | Lightweight, persistent |

**Python dependencies**: Minimal - only `animdl` (pip install) and optional visualization tools

## Architecture Design

### Modular CLI Tools

```
anime-zipf-analysis/
├── Cargo.toml                  # Workspace definition
├── crates/
│   ├── shared/                 # Shared library
│   │   ├── src/
│   │   │   ├── models.rs       # Data models (Anime, Job, JobStatus)
│   │   │   ├── queue.rs        # SQLite job queue interface
│   │   │   ├── db.rs           # Database schema & operations
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── mal-scraper/            # CLI Tool #1
│   │   ├── src/
│   │   │   ├── main.rs         # Entry point
│   │   │   ├── jikan.rs        # Jikan API integration
│   │   │   └── scraper.rs      # Fallback web scraping
│   │   └── Cargo.toml
│   │
│   ├── anime-downloader/       # CLI Tool #2
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   └── animdl.rs       # animdl subprocess wrapper
│   │   └── Cargo.toml
│   │
│   ├── transcriber/            # CLI Tool #3
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── whisper.rs      # whisper-rs integration
│   │   │   └── audio.rs        # Audio extraction
│   │   └── Cargo.toml
│   │
│   ├── tokenizer/              # CLI Tool #4
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   └── vibrato.rs      # vibrato tokenization
│   │   └── Cargo.toml
│   │
│   ├── analyzer/               # CLI Tool #5
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── statistics.rs   # Zipf's law fitting
│   │   │   └── plots.rs        # plotly visualization
│   │   └── Cargo.toml
│   │
│   └── scheduler-tui/          # TUI Application
│       ├── src/
│       │   ├── main.rs
│       │   ├── dashboard.rs    # ratatui UI components
│       │   ├── scheduler.rs    # Job scheduling logic
│       │   └── monitor.rs      # Real-time job monitoring
│       └── Cargo.toml
│
├── models/                     # Whisper model files
│   └── ggml-*.bin
├── data/                       # Data directory (see details below)
├── environment.yml             # Conda environment
└── scripts/
    └── visualize.py            # Optional Python visualization
```

## Implementation Phases

### Phase 1: Project Structure & Shared Library
**Goal**: Set up Cargo workspace and shared job queue infrastructure

**Tasks**:
- Create Cargo workspace with 6 crates
- Design SQLite schema for job tracking
- Implement shared library:
  - Data models (Anime, Episode, Job, JobStatus enums)
  - SQLite job queue API (enqueue, dequeue, update_status, etc.)
  - Error types and utilities
- Set up conda environment for Python dependencies

**Deliverables**:
- `Cargo.toml` workspace
- `shared` crate with job queue implementation
- `environment.yml` for animdl installation

---

### Phase 2: MAL Scraper (CLI Tool #1)
**Goal**: Fetch anime lists from MyAnimeList with auto-discovery and deduplication

**Tasks**:
- Integrate `jikan-rs` for Jikan API v4
- **Auto-discover all categories** with ≥50 anime:
  - Genres (Action, Romance, Comedy, etc.)
  - Explicit Genres (Boys Love, Girls Love, etc.)
  - Themes (School, Military, Isekai, etc.)
  - Demographics (Shounen, Seinen, Josei, Shoujo)
  - Studios (Bones, Kyoto Animation, ufotable, etc.)
- Fetch top 50 anime per category
- **CRITICAL: Deduplicate across categories using `HashSet<mal_id>`**
  - Same anime may appear in multiple categories
  - Use `mal_id` as unique identifier
  - Expected: ~800-1200 unique anime from ~6500-10000 raw entries
- Write deduplicated results to job queue
- Use `get_or_create_anime()` to prevent duplicate anime entries
- Conservative rate limiting: 2 req/s, 50 req/min
- Permanent cache (never expire)

**Expected Scale**:
- Categories processed: ~130-200
- Unique anime: ~800-1200
- Total episodes: ~12,000-18,000

**Deliverables**:
- `mal-scraper` binary
- Populated job queue with deduplicated anime download tasks
- Cache directory (~20 MB)
- Statistics JSON file
- See [MAL_SCRAPER_SPEC.md](./MAL_SCRAPER_SPEC.md) for complete specification

---

### Phase 3: Anime Downloader (CLI Tool #2)
**Goal**: Download anime episodes using animdl

**Tasks**:
- Implement animdl CLI wrapper:
  ```rust
  Command::new("animdl")
      .args(&["download", title, "--range", episodes, "--auto-select"])
  ```
- Poll job queue for `Stage::Queued` jobs
- Download episodes to designated directory
- Update job status with progress
- Error handling and retry logic
- Mark jobs as `Stage::Downloaded` on success

**Deliverables**:
- `anime-downloader` binary
- Downloaded video files in organized structure

---

### Phase 4: Transcriber (CLI Tool #3)
**Goal**: Convert audio to Japanese text using Whisper

**Tasks**:
- Integrate `whisper-rs`:
  - Load model (base/small for speed, large for accuracy)
  - Support CUDA if available
- Extract audio from video files (16kHz WAV format)
- Transcribe with language hint (`--language ja`)
- Post-processing:
  - Detect and remove hallucination patterns
  - Filter repeated segments
- Store transcripts to text files
- **AGGRESSIVE CLEANUP (to keep peak disk < 250GB):**
  - Delete extracted audio file after successful transcription
  - **Delete video file immediately after transcription** (don't wait for tokenizer!)
  - This keeps only ~50 videos in flight at a time (~25 GB)
- Update job status to `Stage::Transcribed`

**Deliverables**:
- `transcriber` binary
- Transcript text files
- Video and audio files automatically deleted (peak disk: ~35 GB)

---

### Phase 5: Tokenizer (CLI Tool #4)
**Goal**: Tokenize Japanese text into words

**Tasks**:
- Integrate `vibrato` with ipadic or unidic dictionary
- Load dictionary once (cache in memory)
- Process transcripts:
  - Tokenize sentences
  - Extract surface forms
  - Filter by POS tags (e.g., keep nouns, verbs, adjectives)
- Generate word frequency lists
- Store tokenized output (JSON/CSV)
- **No cleanup needed**: Video already deleted by transcriber
- Update job status to `Stage::Tokenized`

**Deliverables**:
- `tokenizer` binary
- Tokenized word lists per episode

---

### Phase 6: Analyzer (CLI Tool #5)
**Goal**: Statistical analysis and Zipf's law validation

**Tasks**:
- Aggregate word frequencies across episodes/anime
- Use `polars` for data processing:
  - Calculate global word frequencies
  - Rank words by frequency
  - Compute log-log regression
- Fit Zipf's law using `statrs`:
  ```
  log(frequency) = -alpha * log(rank) + log(C)
  ```
- Generate comparison statistics:
  - Scripted (anime) vs unscripted (livestream)
  - By genre
  - By studio
- Create visualizations:
  - Plotly: interactive log-log plots
  - Export as HTML
- Export results to CSV/Parquet
- Update job status to `Stage::Complete`

**Deliverables**:
- `analyzer` binary
- Analysis results (CSV/Parquet)
- Interactive HTML plots

---

### Phase 7: Scheduler TUI (Monitoring Application)
**Goal**: Real-time job monitoring and scheduling

**Tasks**:
- Build TUI with `ratatui`:
  - Job list view with progress bars
  - Stage indicators (Queued → Downloading → Transcribing → Tokenizing → Analyzing → Complete)
  - Error display with retry options
  - Statistics panel (total jobs, completed, failed)
- Real-time SQLite polling (every 100ms)
- Update UI with `tokio::sync::mpsc` channels
- Keyboard controls:
  - `q`: Quit
  - `r`: Retry failed jobs
  - `p`: Pause/resume scheduler
  - `j/k`: Navigate job list
- Concurrent job management:
  - Semaphores for limiting concurrent downloads/transcriptions
  - Configurable worker counts

**Deliverables**:
- `scheduler-tui` binary
- Interactive dashboard

---

### Phase 8: Visualization (Optional Python)
**Goal**: Publication-quality figures

**Tasks** (if Rust plotly insufficient):
- Python scripts using matplotlib/seaborn:
  - Read CSV results from analyzer
  - Generate publication-ready plots
  - Export as PDF/SVG
- Compare multiple datasets
- Statistical overlays (confidence intervals, fit lines)

**Deliverables**:
- `scripts/visualize.py`
- High-quality figures

---

## Technical Details

For detailed specifications of the job queue system and file structure, see **[TECHNICAL_DETAILS.md](./TECHNICAL_DETAILS.md)**.

**Summary**:
- **Job Queue**: SQLite database with `jobs`, `anime`, `analysis_results`, and `workers` tables
  - UNIQUE constraint on `(anime_id, episode)` prevents duplicate jobs
  - `anime.mal_id` UNIQUE prevents duplicate anime entries
- **Deduplication**: MAL scraper uses `HashSet` to deduplicate anime across categories
  - Expected: ~500 unique anime from ~2500 raw entries (5x reduction)
- **File Structure**: Organized by anime ID with automatic cleanup of temporary files (video/audio)
- **Aggressive Cleanup Strategy** (optimized for 250GB peak):
  - Audio deleted after transcription
  - **Video deleted immediately after transcription** (not after tokenization!)
  - Only transcripts, tokens, and analysis results kept permanently
- **Disk Usage**:
  - Peak: **~35 GB** (with 50 concurrent downloads) ✅ Well under 250 GB target!
  - Scalable: Can increase to 400 concurrent for ~225 GB peak if faster processing needed
  - Long-term: ~8 GB for 500 anime

---

## Key Design Decisions

### 1. Why animdl over ani-cli?
- **ani-cli**: Bash script, difficult to automate (interactive prompts, no structured output)
- **animdl**: Python CLI with better programmatic interface, `--auto-select` flag, reliable exit codes

### 2. Why Rust for everything?
- **Performance**: 10-100x faster than Python (especially polars vs pandas)
- **Type safety**: Catch errors at compile time
- **Single binary deployment**: No Python dependencies except animdl
- **Async**: Tokio for concurrent processing

### 3. Why SQLite for job queue?
- **Lightweight**: Single file, no server needed
- **ACID guarantees**: Reliable job state
- **Concurrency**: Good enough for single-machine pipeline (100-1000 jobs/sec)
- **Upgrade path**: Can migrate to Redis later if needed

### 4. Why vibrato over MeCab CLI?
- **Pure Rust**: No system dependencies
- **Performance**: Faster than MeCab (cache-efficient)
- **Deployment**: Single binary, easier distribution

### 5. Why ratatui for TUI?
- **Modern**: Active development (tui-rs successor)
- **Flexible**: Powerful layout system
- **Async-friendly**: Works seamlessly with tokio

---

## Potential Challenges & Mitigations

### Challenge 1: Whisper Hallucinations
**Issue**: whisper.cpp shows high hallucination rates in benchmarks
**Mitigation**:
- Implement post-processing to detect repeated segments
- Consider `faster-whisper` backend if quality issues persist
- Validate transcripts with heuristics (detect "Thank you for watching" loops)

### Challenge 2: animdl Output Parsing
**Issue**: No structured JSON output from animdl CLI
**Mitigation**:
- Parse stdout with regex patterns
- Monitor exit codes carefully
- Request `--json` flag from maintainers (future)
- Implement robust error detection

### Challenge 3: MAL Rate Limiting
**Issue**: Direct scraping may hit rate limits
**Mitigation**:
- Primary: Use Jikan API (respects MAL's rate limits)
- Implement exponential backoff
- Cache anime metadata aggressively
- Batch requests where possible

### Challenge 4: Large Data Volume
**Issue**: Processing 50+ anime × 12-24 episodes = 600-1200+ videos
**Mitigation**:
- Stream processing (don't load all in memory)
- Use polars lazy evaluation
- Store intermediate results separately
- Implement checkpointing (resume failed jobs)
- Parallel processing with worker pools

### Challenge 5: Disk Space
**Issue**: Video files are large (500MB-2GB per episode)
**Mitigation**:
- **Aggressive automatic cleanup**: Delete video AND audio immediately after transcription
  - Don't wait for tokenization - transcripts are sufficient
- **Deduplication**: ~500 unique anime instead of 2500 (5x reduction in downloads)
- **Controlled concurrency**: Limit to 50 concurrent downloads = ~25 GB video storage
- Peak disk usage: **~35 GB** (well under 250 GB target!) ✅
- Long-term storage: ~8 GB for 500 anime (only transcripts/tokens/analysis)
- Store only tokenized data and analysis results long-term
- Monitor disk space in TUI
- Configurable concurrent processing limit to control peak disk usage
- **Scaling option**: Can increase to 400 concurrent downloads for ~225 GB peak if faster processing needed

---

## Future Enhancements

1. **Distributed processing**: Migrate SQLite queue to Redis for multi-machine workers
2. **Livestream support**: Add yt-dlp integration for YouTube livestreams
3. **Genre comparison**: Automated comparison reports across genres/studios
4. **Web dashboard**: HTTP API + web UI for remote monitoring
5. **ML classification**: Train model to classify scripted vs unscripted from word distributions

---

## References

- **Jikan API**: https://docs.api.jikan.moe/
- **animdl**: https://github.com/justfoolingaround/animdl
- **whisper-rs**: https://codeberg.org/tazz4843/whisper-rs
- **vibrato**: https://github.com/daac-tools/vibrato
- **polars**: https://pola.rs/
- **ratatui**: https://ratatui.rs/

---

*Last updated: 2025-11-06*
*Status: Planning phase - ready for implementation*
