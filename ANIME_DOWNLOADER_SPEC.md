# Anime Downloader Specification

## Overview

The **anime-downloader** is a disk-aware concurrent downloader that retrieves anime episodes using `ani-cli`. It integrates with the anime-selector cache to download the correct anime and automatically pauses when disk space approaches limits.

## Purpose

The downloader bridges the gap between anime metadata (from mal-scraper) and video transcription (for Whisper). It:

1. Reads cached selections from anime-selector
2. Downloads episodes using ani-cli with the correct anime index
3. Monitors disk space and pauses when thresholds are reached
4. Supports concurrent workers for parallel downloads
5. Tracks download progress and automatically retries failures

## Architecture

### Integration Points

```
┌─────────────────┐
│  anime_selection│  (Selection cache from Phase 3)
│  _cache table   │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  anime-downloader                       │
│  ┌────────────────────────────────┐    │
│  │ For each job in queue:         │    │
│  │  1. Read cached selection      │    │
│  │  2. Check disk space           │    │
│  │  3. Download with ani-cli      │    │
│  │     using cached index         │    │
│  │  4. Verify download success    │    │
│  │  5. Update job status          │    │
│  └────────────────────────────────┘    │
└─────────────────────────────────────────┘
         │
         ▼
┌─────────────────┐
│  Downloaded     │
│  Video Files    │
│  (External HDD) │
└─────────────────┘
```

### Components

1. **anime-downloader** (Rust binary)
   - Main orchestration and worker management
   - Disk space monitoring
   - Job queue coordination

2. **ani-cli** (External tool)
   - Shell-based anime downloader
   - Handles AllAnime API and Cloudflare bypass
   - Video source selection and streaming

3. **DiskMonitor** (Shared library)
   - Real-time disk usage tracking
   - Automatic pause/resume logic
   - Cached measurements for performance

## CLI Options

```
anime-downloader [OPTIONS]

Options:
  -c, --config <CONFIG>
      Configuration file path [default: config.toml]

  -w, --workers <WORKERS>
      Number of concurrent download workers [default: 5]
      Recommended: 3-10 depending on disk speed and bandwidth

  --dry-run
      Test mode without actually downloading
      Useful for testing queue processing logic

  --anime-id <ANIME_ID>
      Download only episodes for this specific MAL ID
      Useful for testing or selective downloading

  -v, --verbose
      Enable verbose logging (DEBUG level)

  -h, --help
      Print help information
```

## Configuration

### config.toml

```toml
[data]
root_dir = "data"                           # Local SSD for database/logs
storage_dir = "/media/external/GDA2025"     # External HDD for videos

[disk_management]
hard_limit_gb = 300                         # Absolute maximum disk usage
pause_threshold_gb = 280                    # Pause downloads at this point
resume_threshold_gb = 250                   # Resume downloads when space freed
check_interval_seconds = 30                 # How often to check disk space
cache_duration_seconds = 5                  # Cache disk measurements
max_concurrent_downloads = 5                # Worker pool size

[disk_management.cleanup]
delete_video_after_transcription = true     # Cleanup config (used by transcriber)
delete_audio_after_transcription = true
```

## Usage Examples

### 1. Standard Operation

```bash
# Start with default settings (5 workers)
RUST_LOG=info cargo run --release -p anime-downloader

# With custom worker count
RUST_LOG=info cargo run --release -p anime-downloader -- --workers 10

# Verbose logging for debugging
RUST_LOG=debug cargo run --release -p anime-downloader -- --verbose
```

### 2. Test Mode

```bash
# Dry run (no actual downloads)
cargo run --release -p anime-downloader -- --dry-run

# Test specific anime
cargo run --release -p anime-downloader -- --anime-id 5114 --dry-run
```

### 3. Selective Download

```bash
# Download only episodes for a specific anime
cargo run --release -p anime-downloader -- --anime-id 5114

# This is useful for:
# - Testing download quality for specific anime
# - Re-downloading failed episodes
# - Processing high-priority anime first
```

### 4. Monitor Progress

```bash
# Check download statistics
sqlite3 data/jobs.db "
SELECT stage, COUNT(*) as count
FROM jobs
GROUP BY stage;
"

# Check failed downloads
sqlite3 data/jobs.db "
SELECT anime_title, episode, error_message
FROM jobs
WHERE stage = 'failed'
LIMIT 10;
"

# Check disk usage
du -sh /media/external/GDA2025/videos
```

## Download Workflow

### 1. Job Acquisition

```rust
// Atomic dequeue operation
let job = queue.dequeue_next(JobStage::Queued)?;

// Or with anime filter
let job = queue.dequeue_next_filtered(JobStage::Queued, anime_id)?;
```

### 2. Selection Lookup

```rust
// Get cached selection from anime-selector
let selection = queue.get_selection(job.mal_id)?
    .ok_or_else(|| anyhow!("No cached selection for MAL ID {}", job.mal_id))?;

// Selection contains:
// - selected_index: 1-based index (e.g., 2 means 2nd candidate)
// - selected_title: The anime title that was selected
// - confidence: Selection confidence level
```

### 3. Disk Space Check

```rust
// Check if we should pause
if disk_monitor.should_pause_downloads()? {
    info!("Disk usage above threshold, pausing downloads");
    wait_for_space().await?;
}

// Breakdown shows detailed usage
let breakdown = disk_monitor.get_breakdown()?;
// breakdown.videos_gb
// breakdown.audio_gb
// breakdown.transcripts_gb
```

### 4. Download Execution

```bash
# ani-cli command format
ani-cli -S <selected_index> -e <episode_number> "<anime_title>"

# Example:
ani-cli -S 2 -e 1 "Fullmetal Alchemist: Brotherhood"
```

**Implementation**:
```rust
let output = Command::new("zsh")
    .args(&[
        "-c",
        &format!(
            "ani-cli -S {} -e {} '{}'",
            selection.selected_index,
            job.episode,
            shell_escape(&job.anime_title)
        )
    ])
    .output()?;
```

### 5. Post-Download

```rust
// Get downloaded file path (ani-cli places it in current directory)
let video_path = find_downloaded_video(&job)?;

// Move to organized storage
let target_path = data_paths.video_file(job.mal_id, job.episode);
fs::create_dir_all(target_path.parent().unwrap())?;
fs::rename(&video_path, &target_path)?;

// Get file size
let video_size = fs::metadata(&target_path)?.len();

// Update database
queue.update_job_with_video(job.id, target_path, video_size)?;
queue.update_stage(job.id, JobStage::Downloaded)?;

// Invalidate disk cache
disk_monitor.invalidate_cache();
```

## File Organization

### Video Storage Structure

```
/media/external/GDA2025/videos/
├── 5114/                           # MAL ID: Fullmetal Alchemist Brotherhood
│   ├── metadata.json               # Anime metadata (permanent)
│   └── episodes/
│       ├── ep001.mkv               # Episode 1 (TEMPORARY - deleted after transcription)
│       ├── ep002.mkv
│       └── ...
├── 1535/                           # MAL ID: Death Note
│   ├── metadata.json
│   └── episodes/
│       ├── ep001.mkv
│       └── ...
└── ...
```

### File Naming Convention

- **Directory**: `<mal_id>/episodes/`
- **Filename**: `ep<episode_number:03>.mkv`
  - Example: `ep001.mkv`, `ep012.mkv`, `ep099.mkv`
- **Extension**: `.mkv` (Matroska container, common for anime)

## Disk-Aware Coordination

### Threshold System

```
0 GB                     250 GB      280 GB      300 GB
├────────────────────────┼───────────┼───────────┤
     Normal Operation    │  Resume   │  Pause    │ Hard Limit
                         │           │           │
                         └───────────┴───────────┘
                           Safe Zone   Warning Zone
```

### Pause/Resume Logic

**Pause conditions**:
- Disk usage > `pause_threshold_gb` (280 GB)
- Automatically stops dequeuing new jobs
- Workers finish current downloads, then wait

**Resume conditions**:
- Disk usage < `resume_threshold_gb` (250 GB)
- Transcriber has freed up space by deleting videos
- Workers resume dequeuing

**Implementation**:
```rust
async fn wait_for_space(&self) -> Result<()> {
    info!(
        worker_id = self.worker_id,
        "Waiting for disk space to be freed"
    );

    loop {
        sleep(Duration::from_secs(30)).await;

        if !self.disk_monitor.should_pause_downloads()? {
            info!(
                worker_id = self.worker_id,
                "Disk space freed, resuming downloads"
            );
            break;
        }

        let breakdown = self.disk_monitor.get_breakdown()?;
        debug!(
            worker_id = self.worker_id,
            total_gb = breakdown.usage.total_gb(),
            threshold_gb = self.disk_monitor.pause_threshold_gb(),
            "Still above threshold"
        );
    }

    Ok(())
}
```

## Error Handling

### Retry Strategy

**Automatic retry conditions**:
- Network errors
- Temporary failures (ani-cli errors)
- Missing video file after download

**Retry configuration**:
```sql
-- jobs table
retry_count INTEGER DEFAULT 0,
max_retries INTEGER DEFAULT 3,
error_message TEXT
```

**Implementation**:
```rust
if job.retry_count < job.max_retries {
    // Increment retry and reset to queued
    queue.increment_retry(job.id)?;
    queue.update_stage(job.id, JobStage::Queued)?;
} else {
    // Max retries exceeded, mark as failed
    queue.update_stage_with_error(
        job.id,
        JobStage::Failed,
        format!("{:#}", e)
    )?;
}
```

### Common Errors

**1. No Selection Found**
```
ERROR No cached selection for MAL ID 12345
```
**Cause**: anime-selector hasn't processed this anime yet
**Solution**: Run anime-selector first for complete coverage

**2. ani-cli Failed**
```
ERROR ani-cli returned non-zero exit code: 1
```
**Cause**: Various reasons (no sources, network issues, wrong index)
**Solution**: Check ani-cli output, verify selection is correct

**3. Video Not Found After Download**
```
ERROR Downloaded video not found in expected location
```
**Cause**: ani-cli failed silently or file naming mismatch
**Solution**: Check ani-cli output, may need manual verification

**4. Disk Full**
```
ERROR Failed to write video file: No space left on device
```
**Cause**: Disk filled faster than monitoring could detect
**Solution**: Reduce worker count, check hard_limit_gb setting

## Performance

### Typical Performance

**Single worker**:
- ~5-15 minutes per episode (varies by anime popularity)
- Depends on AllAnime source availability
- Network bandwidth bottleneck

**Multiple workers** (5 concurrent):
- ~25-75 episodes per hour (ideal conditions)
- ~2,000-6,000 episodes per day
- Actual throughput depends on:
  - Network bandwidth
  - ani-cli source availability
  - Disk write speed

### Resource Usage

- **CPU**: Low (mainly I/O bound)
- **Memory**: ~100-200 MB per worker
- **Network**: Limited by bandwidth and ani-cli sources
- **Disk I/O**: Moderate (sequential writes)

### Optimization Tips

1. **Worker count**: Start with 5, increase if network allows
2. **Storage**: Use external HDD for videos (SSD not necessary)
3. **Bandwidth**: Ensure stable connection, consider off-peak hours
4. **Monitoring**: Watch failed downloads, may indicate wrong selections

## Integration with Other Components

### Upstream: anime-selector

**Dependency**: Requires `anime_selection_cache` table populated

**Check if ready**:
```bash
sqlite3 data/jobs.db "
SELECT COUNT(*) as total,
       SUM(CASE WHEN confidence = 'high' THEN 1 ELSE 0 END) as high_conf
FROM anime_selection_cache;
"
```

**If not ready**: Run anime-selector first
```bash
cargo run --release -p anime-selector -- --workers 10
```

### Downstream: transcriber

**Output**: Videos in `storage_dir/videos/<mal_id>/episodes/`

**Handoff**: Updates job stage from `queued` → `downloaded`

**Transcriber pickup**: Dequeues jobs with `stage = 'downloaded'`

## Monitoring and Statistics

### Real-time Monitoring

```bash
# Watch download progress
watch -n 5 'sqlite3 data/jobs.db "
SELECT stage, COUNT(*) as count
FROM jobs
GROUP BY stage
"'

# Check disk usage
watch -n 10 'df -h /media/external/GDA2025'
```

### Statistics Queries

```sql
-- Download success rate
SELECT
    COUNT(CASE WHEN stage = 'downloaded' THEN 1 END) as success,
    COUNT(CASE WHEN stage = 'failed' THEN 1 END) as failed,
    ROUND(100.0 * COUNT(CASE WHEN stage = 'downloaded' THEN 1 END) /
          (COUNT(CASE WHEN stage = 'downloaded' THEN 1 END) +
           COUNT(CASE WHEN stage = 'failed' THEN 1 END)), 2) as success_rate_pct
FROM jobs
WHERE stage IN ('downloaded', 'failed');

-- Average video size
SELECT
    AVG(video_size_bytes) / 1000000 as avg_size_mb,
    MIN(video_size_bytes) / 1000000 as min_size_mb,
    MAX(video_size_bytes) / 1000000 as max_size_mb
FROM jobs
WHERE video_size_bytes IS NOT NULL;

-- Download progress by anime
SELECT
    anime_title,
    COUNT(*) as total_episodes,
    SUM(CASE WHEN stage = 'downloaded' THEN 1 ELSE 0 END) as downloaded,
    SUM(CASE WHEN stage = 'failed' THEN 1 ELSE 0 END) as failed
FROM jobs
GROUP BY anime_id, anime_title
HAVING total_episodes > 0
ORDER BY downloaded DESC
LIMIT 20;
```

## Troubleshooting

### Issue: High Failure Rate

**Symptoms**: Many jobs marked as failed

**Diagnosis**:
```sql
SELECT error_message, COUNT(*) as count
FROM jobs
WHERE stage = 'failed'
GROUP BY error_message
ORDER BY count DESC
LIMIT 5;
```

**Common causes**:
- Wrong selection index (anime-selector error)
- AllAnime source unavailable
- Network issues

**Solutions**:
- Review low-confidence selections
- Retry during off-peak hours
- Manual verification of failed anime

### Issue: Disk Space Management Not Working

**Symptoms**: Downloader doesn't pause despite high disk usage

**Diagnosis**:
```bash
# Check current disk usage
df -h /media/external/GDA2025

# Check configuration
grep -A 5 "disk_management" config.toml
```

**Solutions**:
- Verify `storage_dir` points to correct disk
- Check threshold settings (pause_threshold_gb)
- Ensure transcriber is running to free space

### Issue: Slow Downloads

**Symptoms**: < 5 episodes per hour

**Diagnosis**: Check ani-cli output for bottlenecks

**Solutions**:
- Reduce worker count (may improve per-worker speed)
- Check network bandwidth
- Try different time of day (AllAnime load varies)

## Future Enhancements

1. **Alternative downloaders**: Support for yt-dlp or other tools
2. **Smart queueing**: Priority based on anime popularity or user requests
3. **Resume partial downloads**: Handle interrupted downloads
4. **Bandwidth limiting**: Throttle total bandwidth usage
5. **Source selection**: Prefer specific video sources/qualities
6. **Distributed downloading**: Multiple machines coordinating via database

---

*Last updated: 2025-11-13*
*Status: Implemented and active (Phase 4)*
