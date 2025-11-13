# GDA2025: Zipf's Law Analysis in Video Media

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/)

A research project investigating **Zipf's law** in media content by analyzing word frequency distributions in Japanese anime. This project processes thousands of episodes to validate whether Zipf's law (word frequency ∝ 1/rank) holds in video content.

## Project Overview

**Research Question**: Does Zipf's law apply to video content, and how does it manifest in scripted media (anime)?

**Approach**:
1. Discover and catalog thousands of anime from MyAnimeList
2. Intelligently select correct anime using Claude AI
3. Download episodes with disk-aware coordination
4. Transcribe Japanese audio using Whisper
5. Tokenize Japanese text and analyze word frequencies
6. Validate Zipf's law through statistical analysis

## Current Status

**Phase 1-3: ✅ COMPLETED**
**Phase 4-5: 🔄 RUNNING**
**Phase 6-9: ⏳ PLANNED**

### Implementation Progress

| Phase | Component | Status | Progress |
|-------|-----------|--------|----------|
| 1 | Project Infrastructure | ✅ Complete | 100% |
| 2 | MAL Scraper | ✅ Complete | 100% |
| 3 | Anime Selector (Claude AI) | ✅ Complete | 100% |
| 4 | Anime Downloader | 🔄 Running | 0.16% (274/172,066) |
| 5 | Transcriber (Whisper) | 🔄 Running | 0.01% (25/172,066) |
| 6 | Tokenizer | ⏳ Planned | 0% |
| 7 | Statistical Analyzer | ⏳ Planned | 0% |
| 8 | TUI Monitor | ⏳ Planned | 0% |
| 9 | Visualization | ⏳ Planned | 0% |

### Key Metrics

- **Anime Discovered**: 13,391 unique anime across 130+ categories
- **Episode Jobs**: 172,066 total episodes to process
- **AI Selection Success**: 60% high confidence, 9% medium, 2% low
- **Downloaded**: 274 episodes (~0.16%)
- **Transcribed**: 25 episodes (~0.01%)
- **Database Size**: 49 MB (SQLite)
- **Transcript Data**: ~140 KB (7 anime completed)

## Features

### Implemented ✅

- **Automatic Anime Discovery**: Auto-discovers 130+ categories from MyAnimeList via Jikan API
- **AI-Powered Selection**: Uses Claude Sonnet 3.5 to intelligently match anime titles
- **Disk-Aware Downloads**: Automatically pauses when disk space is low, resumes when space is freed
- **Aggressive Cleanup**: Deletes video and audio files immediately after transcription
- **Concurrent Processing**: Worker pools for parallel downloads and transcriptions
- **Job Queue System**: SQLite-based coordination with retry logic
- **Structured Logging**: Comprehensive logging with `tracing`

### Planned ⏳

- **Japanese Tokenization**: Using `vibrato` (Rust MeCab alternative)
- **Statistical Analysis**: Zipf's law validation with `polars` and `statrs`
- **Interactive Visualization**: Log-log plots with `plotly`
- **TUI Monitor**: Real-time dashboard with `ratatui`

## Technology Stack

**Languages**:
- **Rust** (~5,300 lines) - Performance-critical pipeline components
- **Python** (scripts) - Claude API, Whisper CLI integration

**Key Technologies**:
- **Database**: SQLite (job queue coordination)
- **API Integration**: Jikan API v4 (MAL), Claude Sonnet 3.5, AllAnime API
- **Video Download**: `ani-cli` (via subprocess)
- **Audio Extraction**: FFmpeg
- **Speech-to-Text**: OpenAI Whisper (local, base model)
- **Async Runtime**: Tokio
- **Logging**: `tracing` + `tracing-subscriber`

## Quick Start

### Prerequisites

- Rust (stable)
- Python 3.10+ with miniconda
- FFmpeg
- ani-cli
- 300GB disk space (for video processing)

See [SETUP.md](./SETUP.md) for detailed installation instructions.

### Build and Run

```bash
# Build entire workspace
cargo build --release

# Run MAL scraper (Phase 2) - Already completed
RUST_LOG=info cargo run --release -p mal-scraper

# Run anime selector (Phase 3) - Already completed
RUST_LOG=info cargo run --release -p anime-selector -- --workers 10

# Run downloader (Phase 4) - Currently running
RUST_LOG=info cargo run --release -p anime-downloader -- --workers 5

# Run transcriber (Phase 5) - Currently running
RUST_LOG=info cargo run --release -p transcriber -- --workers 2 --model base
```

### Monitor Progress

```bash
# Check job queue status
sqlite3 data/jobs.db "
SELECT stage, COUNT(*) as count
FROM jobs
GROUP BY stage;
"

# Check disk usage
du -sh /media/external/GDA2025

# View logs
tail -f data/logs/anime-downloader.log
tail -f data/logs/transcriber.log
```

## Project Structure

```
GDA2025/
├── crates/
│   ├── shared/              # Core library (models, database, utilities)
│   ├── mal-scraper/         # MAL anime discovery
│   ├── anime-selector/      # Claude AI selection
│   ├── anime-downloader/    # Download manager
│   └── transcriber/         # Whisper transcription
├── scripts/
│   ├── get_anime_candidates.sh  # AllAnime API query
│   └── select_anime.py          # Claude selection logic
├── data/                    # Data directory (gitignored)
│   ├── jobs.db              # SQLite database (49MB)
│   ├── cache/               # MAL API cache (596KB)
│   ├── transcripts/         # Text transcripts (~140KB)
│   ├── audio/               # Temporary audio (auto-deleted)
│   └── videos/              # Temporary videos (auto-deleted)
├── docs/                    # Additional documentation
├── PLAN.md                  # Complete implementation plan
├── TECHNICAL_DETAILS.md     # Database schema & architecture
├── *_SPEC.md                # Component specifications
├── SETUP.md                 # Installation guide
└── config.toml              # Configuration file
```

## Documentation

- **[PLAN.md](./PLAN.md)** - Complete 9-phase implementation roadmap
- **[TECHNICAL_DETAILS.md](./TECHNICAL_DETAILS.md)** - Database schema and architecture
- **[MAL_SCRAPER_SPEC.md](./MAL_SCRAPER_SPEC.md)** - MAL scraper specification
- **[ANIME_SELECTOR_SPEC.md](./ANIME_SELECTOR_SPEC.md)** - Claude selection workflow
- **[ANIME_DOWNLOADER_SPEC.md](./ANIME_DOWNLOADER_SPEC.md)** - Download system specification
- **[TRANSCRIBER_SPEC.md](./TRANSCRIBER_SPEC.md)** - Transcription system specification
- **[DISK_SPACE_MANAGEMENT.md](./DISK_SPACE_MANAGEMENT.md)** - Storage optimization strategy
- **[SETUP.md](./SETUP.md)** - Installation and setup guide
- **[CLAUDE.md](./CLAUDE.md)** - Claude Code assistant instructions

## Architecture Highlights

### Modular Pipeline

Each phase is an independent Rust binary that:
- Reads jobs from SQLite database
- Processes them concurrently with worker pools
- Updates job status atomically
- Handles failures with automatic retry logic

### Intelligent Disk Management

The system implements aggressive cleanup to process 172K episodes within 300GB:
- **Pause threshold**: 280GB (downloader pauses)
- **Resume threshold**: 250GB (downloader resumes)
- **Immediate cleanup**: Videos and audio deleted after transcription
- **Long-term storage**: Only small text files (~6.9GB for all transcripts)

### AI-Powered Anime Selection

Claude Sonnet 3.5 intelligently selects correct anime from AllAnime search results:
- Distinguishes main series from specials/OVAs
- Validates episode counts
- Provides confidence levels for manual review
- Cost: ~$3 for 13,390 selections

## Performance

- **MAL Discovery**: 13,391 anime in ~2 hours
- **AI Selection**: 13,390 anime in ~6 hours (~$3 cost)
- **Download Speed**: ~25-75 episodes/hour (5 workers, varies by source)
- **Transcription Speed**: ~80-120 episodes/day (2 workers, base model, GPU)

## Known Issues

- **Download failures**: 270/544 downloads failed (49% failure rate)
  - Investigating: Wrong selections, source availability, network issues
- **Bottleneck**: Download phase is slowest (0.16% complete)

## Future Enhancements

- [ ] Alternative downloaders (yt-dlp integration)
- [ ] Hallucination detection for Whisper transcripts
- [ ] Distributed processing with Redis queue
- [ ] Web dashboard for remote monitoring
- [ ] Multi-language support (English, French, etc.)
- [ ] Livestream processing (YouTube integration)

## Research Applications

This pipeline enables research into:
- **Zipf's law validation**: Does 1/rank relationship hold?
- **Scripted content analysis**: How does word distribution differ?
- **Genre comparisons**: Do different genres have different distributions?
- **Studio analysis**: Do studios have linguistic patterns?
- **Vocabulary richness**: How does anime vocabulary compare to natural speech?

## Contributing

This is a research project for academic purposes. Contributions are welcome:

1. Fork the repository
2. Create a feature branch
3. Make your changes (follow Rust style guidelines)
4. Submit a pull request

Please ensure:
- Code compiles with `cargo build`
- Tests pass with `cargo test`
- Code is formatted with `cargo fmt`
- Clippy lints pass with `cargo clippy`

## License

GNU General Public License v3.0 - see [LICENSE](./LICENSE) for details.

This is open-source research code. You are free to use, modify, and distribute it under the terms of the GPL v3.0.

## Acknowledgments

- **MyAnimeList** - Anime metadata via Jikan API
- **Anthropic** - Claude Sonnet 3.5 for intelligent selection
- **OpenAI** - Whisper for speech-to-text
- **ani-cli** - Reliable anime downloader
- **Rust Community** - Excellent ecosystem for performant tooling

## Contact

For questions or collaboration:
- Open an issue on GitHub
- See project documentation in repository

---

*Last updated: 2025-11-13*
*Status: Phase 1-3 Complete | Phase 4-5 Running | Phase 6-9 Planned*
