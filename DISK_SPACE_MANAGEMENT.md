# Disk Space Management Design

## Overview

This document describes the disk space management system that allows the pipeline to process 171,881 episodes within a 250GB storage constraint by coordinating downloads and transcriptions.

## Key Principle

**Dynamic Space Management**: Downloader and transcriber work concurrently. When disk usage approaches the limit, downloader pauses and waits for transcriber to free up space by deleting processed videos.

## Storage Budget

### Target Limits
- **Hard limit**: 300 GB total disk usage
- **Pause threshold**: 280 GB (pause new downloads)
- **Resume threshold**: 250 GB (resume downloads)
- **Safety margin**: 20 GB (for ongoing operations)

### Space Allocation
- **Videos (temporary)**: 200-230 GB (dynamic, deleted after transcription)
- **Audio (temporary)**: Deleted immediately after transcription
- **Transcripts**: ~8 GB (permanent, 171k episodes × 50 KB avg)
- **Tokens**: ~5 GB (permanent)
- **Analysis**: ~2 GB (permanent)
- **Cache**: ~150 MB (permanent)
- **Database**: ~200 MB (permanent)

## Architecture

### Components

```
┌─────────────────┐
│  Space Monitor  │  ← Continuously monitors disk usage
└────────┬────────┘
         │
         ├──→ ┌──────────────┐
         │    │  Downloader  │  ← Pauses at 280GB, resumes at 250GB
         │    └──────┬───────┘
         │           │
         │           ↓ (downloads video)
         │    ┌──────────────┐
         │    │     Jobs     │  ← Stage: downloaded
         │    │   Database   │
         │    └──────┬───────┘
         │           │
         │           ↓ (picks up job)
         │    ┌──────────────┐
         └───→│ Transcriber  │  ← Deletes video+audio after transcription
              └──────────────┘
```

### State Machine

```
queued → downloading → downloaded → transcribing → transcribed → ...
  ↑                       ↓              ↓
  │                  [Video File]   [Deleted]
  │                  [~500MB]
  │
  └── Downloader pauses here if disk > 280GB
```

## Implementation Details

### 1. Space Monitor (Shared Library)

**Location**: `crates/shared/src/disk_monitor.rs`

```rust
pub struct DiskMonitor {
    data_dir: PathBuf,    // Local SSD: audio, transcripts, cache, db
    storage_dir: PathBuf, // External HDD: videos
    hard_limit: u64,      // 300 GB
    pause_threshold: u64,  // 280 GB
    resume_threshold: u64, // 250 GB
}

impl DiskMonitor {
    /// Create monitor with dual-path support (local SSD + external HDD)
    pub fn new(
        data_dir: impl AsRef<Path>,
        storage_dir: impl AsRef<Path>,
        hard_limit_gb: u64,
        pause_threshold_gb: u64,
        resume_threshold_gb: u64,
        cache_duration: Duration,
    ) -> Result<Self>;

    /// Get current disk usage across both directories
    pub fn current_usage(&self) -> Result<DiskUsage>;

    /// Check if downloads should be paused
    pub fn should_pause_downloads(&self) -> Result<bool>;

    /// Check if downloads can resume
    pub fn can_resume_downloads(&self) -> Result<bool>;

    /// Get detailed breakdown of space usage
    pub fn get_breakdown(&self) -> Result<SpaceBreakdown>;
}

pub struct DiskUsage {
    pub total_bytes: u64,
    pub videos_bytes: u64,      // From storage_dir
    pub audio_bytes: u64,       // From data_dir
    pub transcripts_bytes: u64, // From data_dir
    pub tokens_bytes: u64,      // From data_dir
    pub cache_bytes: u64,       // From data_dir
    pub db_bytes: u64,          // From data_dir
}

pub struct SpaceBreakdown {
    pub usage: DiskUsage,
    pub percentage: f64,
    pub available_bytes: u64,
    pub can_download: bool,
}
```

**Implementation**:
- **Dual-path monitoring**: Videos on external HDD, everything else on local SSD
- Use `fs::metadata()` to get file sizes
- Walk through directories recursively to calculate total size
- Cache results for 5 seconds to avoid excessive I/O
- Monitors:
  - `storage_dir/videos/` - External HDD (large temporary files)
  - `data_dir/audio/` - Local SSD (temporary files)
  - `data_dir/transcripts/` - Local SSD (permanent files)
  - `data_dir/cache/` - Local SSD (permanent files)
  - `data_dir/jobs.db` - Local SSD (database)

### 2. Downloader Behavior

**Location**: `crates/anime-downloader/src/main.rs`

**Algorithm**:
```rust
loop {
    // Check disk space before each download
    if disk_monitor.should_pause_downloads()? {
        info!("Disk usage exceeded threshold, pausing downloads");

        // Wait until space is freed
        while !disk_monitor.can_resume_downloads()? {
            tokio::time::sleep(Duration::from_secs(30)).await;

            let usage = disk_monitor.current_usage()?;
            info!(
                current_gb = usage.total_bytes / 1_000_000_000,
                threshold_gb = disk_monitor.resume_threshold / 1_000_000_000,
                "Waiting for transcriber to free up space"
            );
        }

        info!("Disk space freed, resuming downloads");
    }

    // Pick next job from queue
    let job = queue.dequeue_next(Stage::Queued)?;

    // Download video
    download_anime(&job).await?;

    // Update stage to 'downloaded'
    queue.update_stage(job.id, Stage::Downloaded)?;
}
```

**Concurrency**:
- Run multiple downloader workers (configurable, default: 5)
- Each worker checks space independently
- All workers pause when threshold exceeded
- Resume together when space available

### 3. Transcriber Behavior

**Location**: `crates/transcriber/src/main.rs`

**Algorithm**:
```rust
loop {
    // Always prioritize 'downloaded' jobs to free up space
    let job = queue.dequeue_next(Stage::Downloaded)?;

    // Update stage
    queue.update_stage(job.id, Stage::Transcribing)?;

    // Extract audio
    let audio_path = extract_audio(&job.video_path)?;
    let audio_size = fs::metadata(&audio_path)?.len();

    // Transcribe
    let transcript = whisper.transcribe(&audio_path)?;

    // Save transcript
    let transcript_path = save_transcript(&transcript)?;
    let transcript_size = fs::metadata(&transcript_path)?.len();

    // CRITICAL: Delete video and audio immediately
    fs::remove_file(&job.video_path)?;
    fs::remove_file(&audio_path)?;

    info!(
        freed_mb = (job.video_size_bytes + audio_size) / 1_000_000,
        "Deleted video and audio, freed space"
    );

    // Update job with sizes (preserved for statistics)
    queue.update_job(job.id, JobUpdate {
        stage: Stage::Transcribed,
        transcript_path: Some(transcript_path),
        transcript_size_bytes: Some(transcript_size),
        audio_size_bytes: Some(audio_size),
        video_deleted: true,
        audio_deleted: true,
    })?;
}
```

**Concurrency**:
- Run multiple transcriber workers (default: 2-4, depends on GPU)
- Each worker processes independently
- Prioritize `downloaded` stage to free space quickly

### 4. Database Updates

**Add to jobs table**:
```sql
-- Track actual file sizes on disk
ALTER TABLE jobs ADD COLUMN video_size_bytes INTEGER;
ALTER TABLE jobs ADD COLUMN audio_size_bytes INTEGER;

-- Track deletion status
ALTER TABLE jobs ADD COLUMN video_deleted BOOLEAN DEFAULT 0;
ALTER TABLE jobs ADD COLUMN audio_deleted BOOLEAN DEFAULT 0;
```

**Query for space calculation**:
```sql
-- Get total space used by videos (not yet deleted)
SELECT SUM(video_size_bytes)
FROM jobs
WHERE video_deleted = 0 AND video_size_bytes IS NOT NULL;

-- Get total space used by audio (not yet deleted)
SELECT SUM(audio_size_bytes)
FROM jobs
WHERE audio_deleted = 0 AND audio_size_bytes IS NOT NULL;
```

### 5. Configuration

**Add to config.toml**:
```toml
[disk_management]
# Storage limits (in GB)
hard_limit_gb = 250
pause_threshold_gb = 230
resume_threshold_gb = 200

# Monitoring
check_interval_seconds = 30
cache_duration_seconds = 5

# Worker limits
max_concurrent_downloads = 5
max_concurrent_transcriptions = 2

[disk_management.cleanup]
# Aggressive cleanup (delete immediately after stage)
delete_video_after_transcription = true
delete_audio_after_transcription = true
delete_transcript_after_tokenization = false
delete_tokens_after_analysis = false
```

## Monitoring and Logging

### Log Messages

**Downloader**:
```
INFO  Disk usage: 215/250 GB (86%), downloading normally
WARN  Disk usage: 232/250 GB (93%), approaching limit
WARN  Disk usage exceeded threshold (232 GB > 230 GB), pausing downloads
INFO  Waiting for transcriber to free up space (current: 232 GB, need: < 200 GB)
INFO  Disk space freed (195 GB), resuming downloads
```

**Transcriber**:
```
INFO  Transcribing episode, video size: 487 MB
INFO  Transcription complete, deleting video and audio
INFO  Freed 512 MB (video: 487 MB, audio: 25 MB)
INFO  Current disk usage: 198/250 GB (79%)
```

### Metrics to Track

1. **Current disk usage** (GB and %)
2. **Video space** (temporary, should fluctuate)
3. **Transcript space** (grows monotonically)
4. **Download pause events** (count and duration)
5. **Space freed per transcription** (average MB)
6. **Processing rate**:
   - Downloads per hour
   - Transcriptions per hour
   - Net storage growth rate

## Testing Strategy

### Unit Tests
- Test `DiskMonitor` with mock filesystem
- Test pause/resume logic
- Test cleanup operations

### Integration Test
1. Set very low limits (e.g., 2 GB)
2. Download 10 small videos
3. Verify downloader pauses
4. Verify transcriber frees space
5. Verify downloader resumes

### Stress Test
- Run with real limits (250 GB)
- Monitor for 24 hours
- Verify no disk overflow
- Verify efficient space utilization

## Edge Cases

### 1. Transcriber Slower than Downloader
- **Symptom**: Disk fills up, downloads pause frequently
- **Solution**: Reduce concurrent downloads, increase transcription workers

### 2. Transcriber Faster than Downloader
- **Symptom**: No videos to transcribe, disk usage low
- **Solution**: Normal operation, increase downloads if desired

### 3. Disk Full Despite Pause
- **Symptom**: Ongoing downloads complete after pause triggered
- **Solution**: Safety margin (20 GB) handles this

### 4. Transcription Failures
- **Symptom**: Videos not deleted, space not freed
- **Solution**: Retry transcription, manual cleanup if persistent failure

### 5. Power Loss / Crash
- **Symptom**: Incomplete jobs, files not deleted
- **Solution**: On startup, scan for orphaned files and clean up

## Performance Estimates

### Throughput
- **Download speed**: 10 MB/s → 500 MB video in 50 seconds
- **Transcription speed**: 2x realtime → 24 min video in 12 minutes
- **Transcription is bottleneck**: ~5 episodes/hour per GPU

### Space Utilization
- **Steady state**: ~220 GB used (near pause threshold)
- **Videos in flight**: ~40-50 at any time
- **Processing rate**: ~10 episodes/hour (2 GPUs)
- **Total time**: 171,881 episodes ÷ 10/hour = **17,188 hours = 716 days**

**With more aggressive parallelization** (4 GPUs, 8 transcription workers):
- **Processing rate**: ~40 episodes/hour
- **Total time**: 171,881 ÷ 40 = **4,297 hours = 179 days**

## Future Optimizations

1. **Predictive Pausing**: Pause downloads before hitting threshold based on download rate
2. **Priority Queue**: Prioritize small videos when disk is full
3. **Distributed Processing**: Multiple machines with shared queue
4. **Incremental Cleanup**: Delete every N minutes instead of per-file
5. **Compression**: Compress transcripts on-the-fly (save ~50% space)

## Implementation Notes

### Dual-Path Storage Fix (2025-11-13)

**Issue**: Initial implementation monitored only `data_dir`, missing the videos on `storage_dir` (external HDD), causing monitoring to report 4KB instead of 407GB actual usage.

**Fix Applied**:
- Updated `DiskMonitor::new()` to accept both `data_dir` and `storage_dir` parameters
- Modified `calculate_usage()` to read videos from `storage_dir/videos/`
- Updated both `anime-downloader` and `transcriber` to pass correct paths
- Result: Now correctly monitors 437GB total (436.6GB videos + 0.4GB other files)

**Testing**: All unit tests pass, dry-run confirmed correct disk usage detection.

---

**Status**: Implemented and operational (as of Phase 4-5)
**Last updated**: 2025-11-13
