# Anime Selector Specification

## Overview

The **anime-selector** is a CLI tool that pre-selects correct anime titles using Claude Haiku AI before downloading. It solves the common problem where anime download tools return multiple search results (main series, specials, OVAs, recaps), and automatically selecting the first result often downloads the wrong content.

## Purpose

When downloading anime using tools like `ani-cli`, a search query often returns multiple results:
- Main TV series (e.g., "ACCA: 13-ku Kansatsu-ka" - 12 episodes)
- Specials (e.g., "ACCA: 13-ku Kansatsu-ka Specials" - 6 episodes)
- OVAs (e.g., "ACCA: 13-ku Kansatsu-ka - Regards" - 1 episode)
- Recaps, movies, alternative versions, etc.

The anime-selector uses Claude Haiku to intelligently select the correct anime based on MyAnimeList (MAL) metadata, considering:
1. Episode count matching
2. Type (TV series vs Special/OVA)
3. Year and season
4. Title similarity

## Architecture

### Two-Phase Design

**Phase 1: Pre-selection (anime-selector)**
- Query AllAnime API for each anime
- Use Claude Haiku to select the best match
- Cache selections in `anime_selection_cache` table
- Can be run independently before any downloads

**Phase 2: Download (anime-downloader)**
- Read cached selections from database
- Use the selected index directly with `ani-cli`
- No repeated API calls or guessing

This separation allows:
- Manual review of selections before downloading
- Cost control (one-time API cost)
- Batch processing with different worker counts
- Easy correction of low-confidence selections

## How It Works

### Workflow

```
┌─────────────┐
│  MAL Data   │  (From mal-scraper)
│  in SQLite  │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────────────────┐
│  anime-selector                             │
│  ┌────────────────────────────────────┐    │
│  │ For each anime:                     │    │
│  │  1. Query AllAnime API              │    │
│  │     (via get_anime_candidates.sh)   │    │
│  │                                      │    │
│  │  2. Get candidate list              │    │
│  │     ["Special (6 eps)",             │    │
│  │      "Main Series (12 eps)",        │    │
│  │      "OVA (1 eps)"]                 │    │
│  │                                      │    │
│  │  3. Call Claude Haiku               │    │
│  │     (via select_anime.py)           │    │
│  │     - Compare with MAL metadata     │    │
│  │     - Return index, confidence      │    │
│  │                                      │    │
│  │  4. Cache result in database        │    │
│  └────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
       │
       ▼
┌─────────────────┐
│  Selection      │
│  Cache Table    │
│  (SQLite)       │
└─────────────────┘
       │
       ▼
┌─────────────────┐
│ anime-downloader│  (Uses cached index)
└─────────────────┘
```

### Components

1. **anime-selector** (Rust binary)
   - Main orchestration and concurrency control
   - Database operations
   - Statistics tracking

2. **get_anime_candidates.sh** (Bash script)
   - Queries AllAnime GraphQL API
   - Bypasses Cloudflare protection
   - Returns JSON array of candidates

3. **select_anime.py** (Python script)
   - Calls Claude Haiku API
   - Parses response
   - Returns selection with confidence level

## CLI Options

```
anime-selector [OPTIONS]

Options:
  -c, --config <CONFIG>
      Configuration file path [default: config.toml]

  -w, --workers <WORKERS>
      Number of concurrent workers [default: 5]
      Recommended: 5-20 depending on rate limits

  --dry-run
      Test mode without caching selections
      Use this to test the selection logic

  --mal-id <MAL_ID>
      Process only specific MAL ID
      Useful for testing single anime

  --review
      Review mode: show low-confidence selections only
      Use this to check selections that need manual review

  -h, --help
      Print help information
```

## Usage Examples

### 1. Test with a Single Anime

```bash
# Dry-run mode (doesn't cache)
RUST_LOG=info cargo run --release -p anime-selector -- \
  --mal-id 33337 --dry-run

# Real run (caches selection)
RUST_LOG=info cargo run --release -p anime-selector -- \
  --mal-id 33337
```

### 2. Process All Anime

```bash
# Standard processing with 5 workers
RUST_LOG=info cargo run --release -p anime-selector -- --workers 5

# Faster processing with more workers
RUST_LOG=info cargo run --release -p anime-selector -- --workers 10

# Conservative (rate limit friendly)
RUST_LOG=info cargo run --release -p anime-selector -- --workers 3
```

**Estimated Time**:
- 5 workers: ~3-4 hours for 13,383 anime
- 10 workers: ~1.5-2 hours
- 20 workers: ~45-60 minutes

### 3. Review Low-Confidence Selections

After processing, review selections that Claude marked as low confidence:

```bash
cargo run --release -p anime-selector -- --review
```

Output:
```
MAL ID: 12345
Anime: Some Obscure Anime Title
Selected: Some Obscure Anime (13 eps)
Confidence: low
Reason: Multiple possible matches with similar episode counts
```

### 4. Manually Correct a Selection

If a selection is incorrect, update it directly in the database:

```bash
sqlite3 data/jobs.db "
UPDATE anime_selection_cache
SET selected_index = 2,
    selected_title = 'Correct Anime Title (24 eps)',
    confidence = 'high'
WHERE mal_id = 12345
"
```

## Database Schema

### anime_selection_cache Table

```sql
CREATE TABLE anime_selection_cache (
    mal_id INTEGER PRIMARY KEY,
    anime_title TEXT NOT NULL,
    search_query TEXT NOT NULL,
    selected_index INTEGER NOT NULL,      -- 1-based index from candidates
    selected_title TEXT NOT NULL,         -- The selected title
    confidence TEXT NOT NULL              -- 'high', 'medium', or 'low'
        CHECK(confidence IN ('high', 'medium', 'low')),
    reason TEXT,                          -- Why this selection was made
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (mal_id) REFERENCES anime(mal_id)
);

CREATE INDEX idx_selection_cache_confidence
ON anime_selection_cache(confidence);
```

### Selection Fields

- **mal_id**: MyAnimeList ID (primary key)
- **anime_title**: Original title from MAL
- **search_query**: Query used for AllAnime API
- **selected_index**: 1-based index (e.g., 3 means 3rd candidate)
- **selected_title**: Full title of selected anime with episode count
- **confidence**: Selection confidence level
  - `high`: Exact match, very confident
  - `medium`: Good match but some ambiguity
  - `low`: Multiple possibilities, manual review recommended
- **reason**: Explanation from Claude Haiku

## Claude Haiku Selection Criteria

The AI considers these factors in order of importance:

1. **Main series vs Specials/OVA**
   - Strongly prefer TV series over specials/recaps/OVAs
   - Keywords: "Specials", "Recap", "OVA", "ONA" usually indicate non-main content

2. **Episode count matching**
   - Compare with MAL `episodes_total`
   - Tolerance: ±3 episodes is acceptable
   - Large difference (>3) indicates wrong match

3. **Series vs Season**
   - For multi-season anime, match correct season
   - Consider year to identify seasons

4. **Title similarity**
   - Account for romanization variants
   - Consider alternative titles

5. **Year proximity**
   - Should be close to MAL year
   - Within 1-2 years is acceptable

## Output and Statistics

### Real-time Progress

```
[INFO] Starting anime selector
[INFO] Workers: 10
[INFO] Found 13383 anime to process

[INFO] Selecting anime mal_id=33337 title=ACCA: 13-ku Kansatsu-ka
[INFO] Selection complete mal_id=33337 selected="ACCA (12 eps)" confidence=high

[INFO] === Selection Summary ===
[INFO] Total anime: 13383
[INFO] Already cached: 0
[INFO] Newly selected: 13383
[INFO]   - High confidence: 12890
[INFO]   - Medium confidence: 421
[INFO]   - Low confidence: 72
[INFO] Errors: 0
```

### Query Statistics

After processing, check statistics:

```bash
sqlite3 data/jobs.db "
SELECT
  confidence,
  COUNT(*) as count,
  ROUND(COUNT(*) * 100.0 / (SELECT COUNT(*) FROM anime_selection_cache), 1) as percentage
FROM anime_selection_cache
GROUP BY confidence;
"
```

Expected distribution:
- High confidence: ~95-97%
- Medium confidence: ~2-4%
- Low confidence: ~0.5-1%

## Cost Estimation

### Claude Haiku API Pricing
- Input: $0.25 per million tokens
- Output: $1.25 per million tokens

### Per Selection
- Average input: ~100 tokens (MAL metadata + candidates)
- Average output: ~50 tokens (JSON response)
- **Cost per selection**: ~$0.000225

### Total Cost
For 13,383 anime:
- Input tokens: 13,383 × 100 = 1,338,300 tokens
- Output tokens: 13,383 × 50 = 669,150 tokens
- Input cost: $0.33
- Output cost: $0.84
- **Total: ~$1.17**

Note: Actual cost may vary based on:
- Number of candidates per anime
- Complexity of selection reasoning
- API pricing changes

### Cost Optimization
- **Caching**: Selections are cached permanently
- **No re-selection**: Already cached anime are skipped
- **Batch processing**: Run once for all anime
- **Manual review**: Fix low-confidence selections without re-running

## Best Practices

### 1. Test First
Always test with a few anime before processing everything:
```bash
# Test 5 different anime
for mal_id in 33337 5114 1 20 30; do
  cargo run --release -p anime-selector -- --mal-id $mal_id
done

# Review results
sqlite3 data/jobs.db "SELECT * FROM anime_selection_cache"
```

### 2. Monitor Progress
Use INFO logging to track progress:
```bash
RUST_LOG=info cargo run --release -p anime-selector -- --workers 10 \
  2>&1 | tee selection_log.txt
```

### 3. Handle Interruptions
The tool is resumable - if interrupted, re-run and it will skip already cached selections:
```bash
# First run (interrupted)
RUST_LOG=info cargo run --release -p anime-selector -- --workers 10

# Resume (skips cached entries)
RUST_LOG=info cargo run --release -p anime-selector -- --workers 10
```

### 4. Review Low-Confidence
After completion, always review low-confidence selections:
```bash
# Show low-confidence selections
cargo run --release -p anime-selector -- --review

# Check count
sqlite3 data/jobs.db "
SELECT COUNT(*) FROM anime_selection_cache WHERE confidence='low'
"
```

### 5. Backup Before Corrections
Before manually correcting selections:
```bash
# Backup database
cp data/jobs.db data/jobs.db.backup

# Make corrections
sqlite3 data/jobs.db "UPDATE anime_selection_cache SET ..."
```

## Troubleshooting

### No Candidates Found
```
ERROR Failed to get candidates error="No candidates found"
```

**Cause**: AllAnime doesn't have this anime, or search query doesn't match.

**Solution**:
1. Check if anime exists on AllAnime
2. Try alternative title (English vs Japanese)
3. Manually specify correct title in database

### API Authentication Error
```
ERROR API call failed: authentication_error
```

**Cause**: Invalid or expired Anthropic API key.

**Solution**:
1. Check `config.toml` has valid API key
2. Get new key from https://console.anthropic.com/
3. Update `[anthropic] api_key` field

### Python Module Not Found
```
ERROR Failed to execute select_anime.py
ModuleNotFoundError: No module named 'anthropic'
```

**Cause**: Python script using wrong environment.

**Solution**:
1. Ensure GDA2025 conda environment exists
2. Install anthropic: `conda activate GDA2025 && pip install anthropic`
3. Code uses hardcoded path: `/home/yuc/miniconda3/envs/GDA2025/bin/python3`

### Rate Limiting
If AllAnime API rate limits you:
1. Reduce worker count: `--workers 3`
2. The tool will retry automatically
3. Consider adding delays in `get_anime_candidates.sh`

## Integration with Downloader

After running anime-selector, the downloader uses cached selections:

```rust
// In anime-downloader
let selection = queue.get_selection(job.mal_id)?;

let output = Command::new("ani-cli")
    .args(&[
        "-S", &selection.selected_index.to_string(),  // Use cached index!
        "-e", &format!("{}", job.episode),
        &job.anime_title
    ])
    .output()?;
```

Benefits:
- No guessing or auto-select first result
- Consistent downloads across all episodes
- Fast lookup (database query vs API call)

## Performance

### Typical Run Times
- Single anime (cold): ~4-5 seconds
- Single anime (cached): ~0.1 seconds
- 100 anime with 5 workers: ~8-10 minutes
- 13,383 anime with 10 workers: ~1.5-2 hours

### Resource Usage
- CPU: Low (mainly I/O bound)
- Memory: ~50-100 MB
- Network: API calls only
- Disk: Negligible (database entries ~1 KB each)

### Bottlenecks
1. **Claude Haiku API latency**: ~2-4 seconds per request
2. **AllAnime API**: ~0.5-1 second per query
3. **Rate limits**: Both APIs may have limits

## Future Enhancements

1. **Model Selection**: Allow choosing different Claude models
2. **Custom Prompts**: User-defined selection criteria
3. **Fallback Logic**: Handle cases where no candidates found
4. **Bulk Export**: Export selections to CSV for review
5. **Confidence Tuning**: Adjust confidence thresholds
6. **Retry Failed**: Automatic retry of error cases

---

*Last updated: 2025-11-10*
