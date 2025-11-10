# Undecided Technical Details

This document lists all technical decisions that still need to be made before implementation.

---

## 1. MAL Scraper

### 1.1 Category Lists
**Issue**: Which specific categories to scrape?

**Questions**:
- [ ] Which Genres to include? (Action, Romance, Comedy, Drama, etc. - full list?)
- [ ] Which Explicit Genres? (Boys Love, Girls Love, etc.)
- [ ] Which Themes? (Military, School, Isekai, etc.)
- [ ] Which Studios? (Bones, Kyoto Animation, ufotable, etc. - top N studios?)
- [ ] Do we filter categories by item count (e.g., skip categories with <50 anime)?

**Current status**: Strategy defined, but no concrete lists

---

### 1.2 Rate Limiting
**Issue**: How to handle Jikan API rate limits?

**Questions**:
- [ ] What are Jikan API rate limits? (requests per second/minute?)
- [ ] Exponential backoff strategy? (initial delay, max delay, backoff factor?)
- [ ] Concurrent request limit?
- [ ] Retry logic for 429 (Too Many Requests)?

**Current status**: Mentioned but not specified

---

### 1.3 Caching
**Issue**: How long to cache MAL metadata?

**Questions**:
- [ ] Cache duration? (1 day? 1 week? permanent?)
- [ ] Cache invalidation strategy?
- [ ] What to do if anime metadata changes (e.g., episode count updated)?

**Current status**: Cache directory exists in file structure but no policy defined

---

## 2. Anime Downloader

### 2.1 Video Quality/Format
**Issue**: What video quality and format to download?

**Questions**:
- [ ] Target video quality? (720p, 1080p, best available?)
- [ ] Preferred container format? (MKV, MP4, etc.)
- [ ] Codec preferences? (H.264, H.265, etc.)
- [ ] File size considerations? (balance between quality and disk space)
- [ ] Subtitle handling? (download subtitles? which languages? needed for analysis?)

**Current status**: Using `--auto-select` and `--quality best` but specifics unclear

---

### 2.2 Download Validation
**Issue**: How to verify downloaded files are valid?

**Questions**:
- [ ] Check file size (minimum/maximum thresholds)?
- [ ] Verify video codec/container?
- [ ] Check duration matches expected episode length?
- [ ] Checksum verification (if available from source)?
- [ ] What to do with partial/corrupted downloads?

**Current status**: Not addressed

---

### 2.3 Episode Number Handling
**Issue**: How to handle episode numbering edge cases?

**Questions**:
- [ ] Special episodes (OVA, specials)? Include or skip?
- [ ] Episode 0 (prologues)? Include or skip?
- [ ] Multi-part episodes (ep 1a, 1b)?
- [ ] How to map animdl episode numbers to MAL episode numbers?

**Current status**: Assumes simple 1-N numbering

---

### 2.4 Download Failures
**Issue**: How to handle download failures?

**Questions**:
- [ ] Max retry attempts for failed downloads?
- [ ] Retry delay strategy?
- [ ] Skip episode vs. fail entire anime?
- [ ] Fallback sources if animdl fails?
- [ ] Manual intervention workflow?

**Current status**: Error handling mentioned but not specified

---

## 3. Transcriber (Whisper)

### 3.1 Model Selection
**Issue**: Which Whisper model to use?

**Questions**:
- [ ] Model size: tiny/base/small/medium/large/large-v3?
  - tiny: fastest, lowest accuracy
  - base: balanced
  - small: better accuracy
  - medium: high accuracy, slower
  - large/large-v3: best accuracy, very slow
- [ ] Quantized models? (for faster inference)
- [ ] Different models for different stages (quick pass + refinement)?
- [ ] Trade-off between speed and accuracy for 9000 episodes?

**Current status**: "base/small for speed, large for accuracy" - not decided

---

### 3.2 Audio Extraction
**Issue**: How to extract audio from video?

**Questions**:
- [ ] Tool to use? (ffmpeg most likely, but confirm)
- [ ] Audio format: WAV 16kHz mono (confirmed), but codec?
- [ ] Sample rate conversion quality?
- [ ] Handle stereo → mono conversion how? (left channel? mix?)
- [ ] Audio normalization needed?
- [ ] Extract full episode or segment-by-segment?

**Current status**: "16kHz WAV" specified but tool/process not detailed

---

### 3.3 Transcription Parameters
**Issue**: Whisper configuration settings?

**Questions**:
- [ ] Language parameter: `ja` (Japanese) confirmed, but dialect?
- [ ] Temperature parameter for sampling?
- [ ] Beam size for search?
- [ ] Best_of parameter for quality?
- [ ] Initial prompt to improve accuracy? (e.g., anime-specific vocabulary)
- [ ] Word timestamps needed? (for analysis?)
- [ ] Compression ratio threshold (for hallucination detection)?

**Current status**: Only `--language ja` mentioned

---

### 3.4 Hallucination Detection
**Issue**: How to detect and remove hallucinations?

**Questions**:
- [ ] Specific patterns to detect? (repeated phrases, "Thank you for watching", etc.)
- [ ] Algorithms:
  - Repetition detection (sliding window? threshold?)
  - Compression ratio check?
  - Entropy-based filtering?
  - Confidence score threshold?
- [ ] Manual review needed for edge cases?
- [ ] Log detected hallucinations for analysis?

**Current status**: "Detect and remove hallucination patterns" - no algorithm specified

---

### 3.5 GPU/CPU Configuration
**Issue**: GPU acceleration setup?

**Questions**:
- [ ] CUDA support: yes/no? (whisper-rs supports it)
- [ ] Fallback to CPU if GPU unavailable?
- [ ] GPU memory management (batch size)?
- [ ] Multi-GPU support needed?
- [ ] Benchmark GPU vs CPU speed difference?

**Current status**: "Support CUDA if available" - no configuration details

---

### 3.6 Non-Japanese Content
**Issue**: How to handle non-Japanese audio (OP/ED songs, English phrases)?

**Questions**:
- [ ] Detect language switches?
- [ ] Use multi-language Whisper model?
- [ ] Filter out non-Japanese segments?
- [ ] Include romaji/English in analysis or skip?

**Current status**: "Japanese language content only" but no handling of mixed content

---

## 4. Tokenizer

### 4.1 Dictionary Selection
**Issue**: Which MeCab/vibrato dictionary to use?

**Questions**:
- [ ] Dictionary options:
  - ipadic (standard, older)
  - unidic (modern, more detailed POS tags)
  - mecab-ipadic-neologd (includes recent slang/names)
  - unidic-cwj (contemporary written Japanese)
- [ ] Dictionary size vs. accuracy trade-off?
- [ ] Anime-specific vocabulary coverage? (character names, fantasy terms)
- [ ] Update frequency for neologisms?

**Current status**: "ipadic or unidic" - not decided

---

### 4.2 POS Tag Filtering
**Issue**: Which parts of speech to include in analysis?

**Questions**:
- [ ] Keep: nouns, verbs, adjectives (confirmed), but which subcategories?
- [ ] Particles (助詞): include or exclude? (very frequent, may dominate Zipf curve)
- [ ] Auxiliary verbs (助動詞): include or exclude?
- [ ] Symbols, punctuation: exclude?
- [ ] Proper nouns (人名, 地名): separate analysis?
- [ ] Unknown words: how to handle?

**Current status**: "Filter by POS tags (e.g., keep nouns, verbs, adjectives)" - not specific

---

### 4.3 Normalization
**Issue**: Text normalization strategy?

**Questions**:
- [ ] Katakana → Hiragana conversion? (e.g., コレ → これ)
- [ ] Kanji variants (新字体 vs 旧字体)?
- [ ] Half-width vs full-width characters?
- [ ] Case sensitivity (for romaji/English)?
- [ ] Lemmatization? (use base form vs. surface form)
  - e.g., "食べる" (eat) vs "食べた" (ate) → count as same word?

**Current status**: Not addressed

---

### 4.4 Multi-word Expressions
**Issue**: How to handle compound words and phrases?

**Questions**:
- [ ] Treat compound nouns as single unit? (e.g., "鋼の錬金術師")
- [ ] N-gram analysis? (unigrams only, or bigrams/trigrams too?)
- [ ] Collocation detection?
- [ ] Idiomatic expressions?

**Current status**: Not addressed (implies unigram analysis only)

---

## 5. Analyzer

### 5.1 Zipf's Law Fitting
**Issue**: Specific algorithm for fitting power law?

**Questions**:
- [ ] Method:
  - Log-log linear regression (simple, current plan)
  - Maximum likelihood estimation (more accurate)
  - Kolmogorov-Smirnov test for goodness-of-fit?
- [ ] Minimum word frequency threshold (avoid noise from rare words)?
- [ ] Rank range to fit? (top 100? top 1000? all words?)
- [ ] Handle ties in frequency ranking?

**Current status**: "log-log linear regression" mentioned but no details

---

### 5.2 Stop Words
**Issue**: How to handle stop words?

**Questions**:
- [ ] Use stop word list? (Japanese common words: の, は, が, etc.)
- [ ] Custom stop word list for anime? (character names, common phrases)
- [ ] Compare Zipf curves with/without stop words?
- [ ] Impact on vocabulary richness calculation?

**Current status**: Not addressed

---

### 5.3 Statistical Comparison
**Issue**: How to compare different categories statistically?

**Questions**:
- [ ] Statistical tests:
  - T-test for alpha (Zipf exponent) differences?
  - Chi-square test for distribution differences?
  - Permutation tests?
  - Effect size measures?
- [ ] Significance level (p < 0.05)?
- [ ] Multiple comparison correction (Bonferroni, FDR)?
- [ ] Confidence intervals?

**Current status**: "Comparative analysis" mentioned but no statistical methods

---

### 5.4 Visualization Details
**Issue**: What plots to generate?

**Questions**:
- [ ] Required plots:
  - Log-log frequency-rank plot (confirmed)
  - Q-Q plot for Zipf distribution?
  - Residuals plot?
  - Histogram of word frequencies?
  - Comparison plots across categories?
- [ ] Plot styling (colors, labels, legends)?
- [ ] Interactive features (tooltips, zoom, pan)?
- [ ] Export formats (HTML, PNG, SVG, PDF)?
- [ ] Annotation (fitted line, R² value, parameters)?

**Current status**: "Interactive log-log plots" - minimal detail

---

### 5.5 Aggregation Strategy
**Issue**: How to aggregate word frequencies across episodes?

**Questions**:
- [ ] Per-episode analysis + aggregate, or aggregate-first?
- [ ] Weighted average by episode length?
- [ ] Handle missing episodes?
- [ ] Per-anime vs. per-category vs. global analysis?

**Current status**: Implied but not specified

---

## 6. Scheduler/TUI

### 6.1 UI Layout
**Issue**: Specific TUI layout and components?

**Questions**:
- [ ] Screen sections:
  - Job list (confirmed)
  - Progress bars (confirmed)
  - Statistics panel (confirmed)
  - Log viewer?
  - System resource monitor (CPU, GPU, disk)?
- [ ] Keybindings:
  - q: quit (confirmed)
  - r: retry (confirmed)
  - p: pause (confirmed)
  - j/k: navigate (confirmed)
  - Other keys? (filter, sort, search, help screen)
- [ ] Color scheme?
- [ ] Refresh rate (100ms confirmed, but configurable)?

**Current status**: Basic features listed but no detailed mockup

---

### 6.2 Worker Management
**Issue**: How to spawn and manage worker processes?

**Questions**:
- [ ] Worker architecture:
  - Separate processes? (safer, isolated)
  - Threads? (lighter weight)
  - Tokio tasks? (async)
- [ ] Worker crash handling:
  - Auto-restart?
  - Exponential backoff for repeated crashes?
  - Alert user?
- [ ] Worker health checks (heartbeat frequency)?
- [ ] Graceful shutdown (wait for current job or cancel)?

**Current status**: Worker table in schema but no process management details

---

### 6.3 Logging
**Issue**: Where and how to log events?

**Questions**:
- [ ] Log file location? (data/logs/?)
- [ ] Log rotation policy? (daily? size-based?)
- [ ] Log levels (trace, debug, info, warn, error)?
- [ ] Log format (JSON? structured?)
- [ ] Integration with TUI (log viewer pane)?
- [ ] Separate logs per worker?

**Current status**: Not addressed

---

## 7. Configuration

### 7.1 Configuration File
**Issue**: Configuration format and location?

**Questions**:
- [ ] Format: TOML (mentioned), JSON, YAML?
- [ ] Location: `config.toml`, `.env`, command-line args, or all three?
- [ ] Configuration precedence (CLI > env > file)?
- [ ] Example configuration provided?
- [ ] Validation of config values?

**Current status**: TOML mentioned in TECHNICAL_DETAILS.md but no schema

---

### 7.2 Configurable Parameters
**Issue**: Which parameters should be configurable?

**Questions**:
- [ ] Paths (data dir, models dir, cache dir)?
- [ ] Concurrency limits (confirmed: downloads, transcriptions, tokenizations)?
- [ ] Whisper model selection?
- [ ] Tokenizer dictionary path?
- [ ] Retry policies (max attempts, delays)?
- [ ] API rate limits?
- [ ] Cleanup behavior (aggressive vs. conservative)?
- [ ] Log verbosity?

**Current status**: Some mentioned, no comprehensive list

---

### 7.3 Default Values
**Issue**: What are sensible defaults?

**Questions**:
- [ ] Max concurrent downloads: 50 (confirmed), but adjust based on hardware?
- [ ] Max concurrent transcriptions: 4 (confirmed), but auto-detect GPU count?
- [ ] Whisper model default: base? small?
- [ ] Dictionary default: ipadic? unidic?
- [ ] Retry max attempts: 3 (confirmed)?

**Current status**: Some defaults in TECHNICAL_DETAILS.md but incomplete

---

## 8. Error Handling & Retry

### 8.1 Retry Strategy
**Issue**: Detailed retry policy?

**Questions**:
- [ ] Which errors are retryable?
  - Network errors: yes
  - File not found: no
  - Transcription errors: depends on type?
  - Parsing errors: no?
- [ ] Retry delay:
  - Fixed delay? (e.g., 5 seconds)
  - Exponential backoff? (e.g., 2^n seconds)
  - Jitter to avoid thundering herd?
- [ ] Per-stage retry limits? (different for download vs. transcription?)

**Current status**: "max_retries = 3" but no delay/backoff strategy

---

### 8.2 Failure Notifications
**Issue**: How to notify user of failures?

**Questions**:
- [ ] TUI alerts (popup, status bar)?
- [ ] Desktop notifications?
- [ ] Email/webhook for critical failures?
- [ ] Summary report at end?

**Current status**: TUI shows error states but no notification system

---

## 9. Data Quality

### 9.1 Quality Validation
**Issue**: How to validate transcript quality?

**Questions**:
- [ ] Manual spot-checking process?
- [ ] Automated quality metrics:
  - Transcription confidence scores?
  - Character-level error rate (if ground truth available)?
  - Vocabulary diversity (detect gibberish)?
- [ ] Sample size for validation? (e.g., 1% of episodes)
- [ ] Thresholds for rejecting poor transcripts?

**Current status**: Hallucination detection mentioned but no broader quality assurance

---

### 9.2 Background Noise/Music
**Issue**: How to handle non-speech audio?

**Questions**:
- [ ] Voice Activity Detection (VAD) to skip silence/music?
- [ ] Filter background music using:
  - Pre-processing (audio separation)?
  - Post-processing (remove music-like tokens)?
  - Accept as noise in data?
- [ ] Impact on Zipf analysis?

**Current status**: Not addressed

---

### 9.3 Character Names
**Issue**: How to handle character names in analysis?

**Questions**:
- [ ] Include character names in word frequency?
- [ ] Treat as proper nouns (separate category)?
- [ ] Map variant names to canonical form? (e.g., "エドワード" and "エド" → same person)
- [ ] Impact on Zipf curve (names may be very frequent)?

**Current status**: Not addressed

---

## 10. Testing

### 10.1 Testing Strategy
**Issue**: How to test the system?

**Questions**:
- [ ] Unit tests for each module? (required? coverage target?)
- [ ] Integration tests:
  - End-to-end pipeline with small dataset?
  - Mock external services (MAL API, animdl)?
- [ ] TUI testing strategy? (snapshot tests? manual?)
- [ ] Performance benchmarks?
- [ ] Continuous integration (CI)?

**Current status**: "Write unit tests alongside code" but no strategy

---

### 10.2 Test Data
**Issue**: Test dataset for development?

**Questions**:
- [ ] Use real anime data or synthetic?
- [ ] Small test set (1-2 anime, 3-5 episodes)?
- [ ] Ground truth transcripts for validation?
- [ ] Committed to repo or downloaded separately?

**Current status**: Not addressed

---

## 11. Deployment

### 11.1 Build Process
**Issue**: How to build and distribute?

**Questions**:
- [ ] Build targets:
  - Linux (confirmed: primary)
  - macOS (mentioned in user prefs)
  - Windows (mentioned in user prefs)
- [ ] Cross-compilation strategy?
- [ ] Static vs. dynamic linking?
- [ ] Binary release process (GitHub Releases)?
- [ ] Package managers (cargo install, apt, homebrew)?

**Current status**: Development environment specified but no build/release plan

---

### 11.2 Dependency Installation
**Issue**: How to install external dependencies?

**Questions**:
- [ ] Whisper model download:
  - Auto-download on first run?
  - Manual download with instructions?
  - Bundled with release?
- [ ] vibrato dictionary:
  - Auto-download?
  - Manual installation?
- [ ] animdl installation instructions (pip install animdl)
- [ ] ffmpeg requirement (for audio extraction)?
- [ ] Conda environment setup automation?

**Current status**: Dependencies listed but no installation automation

---

### 11.3 Documentation
**Issue**: What documentation to provide?

**Questions**:
- [ ] README with:
  - Installation instructions
  - Quick start guide
  - Usage examples
- [ ] API documentation (rustdoc)?
- [ ] Architecture documentation (diagrams)?
- [ ] Troubleshooting guide?
- [ ] Configuration reference?

**Current status**: Proposal and plans exist but no user-facing docs

---

## 12. Performance

### 12.1 Benchmarking
**Issue**: Expected performance metrics?

**Questions**:
- [ ] Whisper transcription speed:
  - Real-time factor? (e.g., 1 minute of audio = 30 seconds to transcribe)
  - GPU vs CPU difference?
  - Different model sizes?
- [ ] tokenization speed (tokens per second)?
- [ ] Overall pipeline throughput (episodes per hour)?
- [ ] Memory usage per component?
- [ ] Database query performance (job queue operations)?

**Current status**: "Processing Rate: 120 episodes/hour" estimated but not benchmarked

---

### 12.2 Resource Limits
**Issue**: Memory and CPU constraints?

**Questions**:
- [ ] Maximum memory usage per worker?
- [ ] Whisper model memory requirements (varies by size)?
- [ ] Polars DataFrame memory limits (lazy evaluation helps)?
- [ ] OOM (Out of Memory) handling?
- [ ] CPU affinity for workers?

**Current status**: Not addressed

---

## 13. Livestream Integration (Future)

### 13.1 yt-dlp Integration
**Issue**: How to integrate livestream downloads? (Future phase)

**Questions**:
- [ ] yt-dlp command-line wrapper (similar to animdl)?
- [ ] Playlist/channel handling?
- [ ] Quality selection?
- [ ] Subtitle download?
- [ ] Age-restricted content handling?

**Current status**: Mentioned in proposal but not in current implementation plan

---

### 13.2 Content Selection
**Issue**: Which livestreams to analyze?

**Questions**:
- [ ] Categories to scrape (gaming, vtubers, etc.)?
- [ ] Language detection (Japanese only)?
- [ ] Minimum duration threshold?
- [ ] Live vs. VOD?

**Current status**: Not addressed (future work)

---

## Summary

**Total undecided items**: ~130+ individual questions across 13 major areas

**Highest priority** (blocking implementation):
1. ✅ MAL category lists (which genres/themes/studios)
2. ✅ Whisper model selection (size/speed trade-off)
3. ✅ vibrato dictionary choice (ipadic vs unidic vs neologd)
4. ✅ POS tag filtering rules (which words to keep)
5. ✅ Audio extraction tool (ffmpeg confirmed but config needed)
6. ✅ Configuration file format and schema
7. ✅ Error retry strategy details

**Medium priority** (affects quality):
8. Hallucination detection algorithm
9. Text normalization rules
10. Statistical comparison methods
11. Quality validation process

**Lower priority** (nice to have):
12. TUI detailed layout
13. Testing strategy
14. Deployment/packaging

---

*This document should be updated as decisions are made. Move decided items to TECHNICAL_DETAILS.md.*
