# Transcriber Specification

## Overview

The **transcriber** converts anime video audio to Japanese text using OpenAI Whisper. It implements aggressive cleanup strategies to minimize disk usage by immediately deleting video and audio files after successful transcription.

## Purpose

The transcriber is a critical component in the data pipeline that:

1. Extracts audio from downloaded video files
2. Transcribes Japanese speech to text using Whisper
3. Saves transcript files for later tokenization
4. **Aggressively deletes video and audio files** to free disk space
5. Enables the analysis pipeline to process thousands of episodes

## Architecture

### Pipeline Integration

```
┌─────────────────┐
│  Downloaded     │
│  Videos         │
│  (External HDD) │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  transcriber                            │
│  ┌────────────────────────────────┐    │
│  │ For each downloaded video:     │    │
│  │  1. Extract audio (FFmpeg)     │    │
│  │  2. Transcribe (Whisper CLI)   │    │
│  │  3. Save transcript            │    │
│  │  4. DELETE video immediately   │    │
│  │  5. DELETE audio immediately   │    │
│  │  6. Update job status          │    │
│  └────────────────────────────────┘    │
└─────────────────────────────────────────┘
         │
         ▼
┌─────────────────┐
│  Transcripts    │
│  (Text files)   │
│  ~30-50 KB each │
└─────────────────┘
```

### Components

1. **transcriber** (Rust binary)
   - Worker pool management
   - Job queue coordination
   - Cleanup orchestration

2. **FFmpeg** (External tool)
   - Audio extraction from video
   - Converts to 16kHz mono WAV format
   - Optimized for Whisper input

3. **Whisper CLI** (Python/openai-whisper)
   - Speech-to-text transcription
   - Language detection and Japanese hints
   - Text output generation

4. **DiskMonitor** (Shared library)
   - Tracks freed disk space
   - Cache invalidation after cleanup

## CLI Options

```
transcriber [OPTIONS]

Options:
  -c, --config <CONFIG>
      Configuration file path [default: config.toml]

  -w, --workers <WORKERS>
      Number of concurrent transcription workers [default: 2]
      Recommended: 1-4 depending on CPU/GPU capacity
      Note: Whisper is CPU/GPU intensive

  -m, --model <MODEL>
      Whisper model to use [default: base]
      Options: tiny, base, small, medium, large
      Trade-off: Speed vs Accuracy
        - tiny:   Fast but lower accuracy (~32x realtime)
        - base:   Good balance (~16x realtime)
        - small:  Better accuracy (~6x realtime)
        - medium: High accuracy (~2x realtime)
        - large:  Best accuracy (~1x realtime)

  --dry-run
      Test mode without actually transcribing
      Useful for testing extraction and cleanup logic

  -v, --verbose
      Enable verbose logging (DEBUG level)

  -h, --help
      Print help information
```

## Configuration

### config.toml

```toml
[data]
root_dir = "data"                           # Local SSD for transcripts
storage_dir = "/media/external/GDA2025"     # External HDD for videos

[disk_management]
max_concurrent_transcriptions = 2           # Worker pool size

[disk_management.cleanup]
delete_video_after_transcription = true     # ⚠️ AGGRESSIVE: Delete video immediately
delete_audio_after_transcription = true     # ⚠️ AGGRESSIVE: Delete audio immediately
delete_transcript_after_tokenization = false # Keep transcripts (small files)
delete_tokens_after_analysis = false        # Keep tokens (needed for analysis)
```

## Usage Examples

### 1. Standard Operation

```bash
# Start with default settings (2 workers, base model)
RUST_LOG=info cargo run --release -p transcriber

# With custom worker count
RUST_LOG=info cargo run --release -p transcriber -- --workers 4

# With different Whisper model
RUST_LOG=info cargo run --release -p transcriber -- --model small
```

### 2. Model Selection

```bash
# Fast processing (lower accuracy)
cargo run --release -p transcriber -- --model tiny

# Balanced (recommended)
cargo run --release -p transcriber -- --model base

# High accuracy (slower)
cargo run --release -p transcriber -- --model large
```

### 3. Testing

```bash
# Dry run (no transcription)
cargo run --release -p transcriber -- --dry-run

# Verbose logging
RUST_LOG=debug cargo run --release -p transcriber -- --verbose
```

### 4. Monitor Progress

```bash
# Check transcription statistics
sqlite3 data/jobs.db "
SELECT stage, COUNT(*) as count
FROM jobs
WHERE stage IN ('downloaded', 'transcribing', 'transcribed')
GROUP BY stage;
"

# Check transcript file sizes
du -sh data/transcripts/*/

# Monitor disk space freed
watch -n 10 'df -h /media/external/GDA2025'
```

## Transcription Workflow

### Step 1: Audio Extraction

**Tool**: FFmpeg

**Command**:
```bash
ffmpeg -i video.mkv \
       -vn \                    # No video
       -acodec pcm_s16le \      # 16-bit PCM
       -ar 16000 \              # 16 kHz sample rate
       -ac 1 \                  # Mono audio
       output.wav
```

**Output format**: 16kHz mono WAV (Whisper optimal format)

**Implementation**:
```rust
let output = Command::new("ffmpeg")
    .args(&[
        "-i", video_path.to_str().unwrap(),
        "-vn",
        "-acodec", "pcm_s16le",
        "-ar", "16000",
        "-ac", "1",
        "-y",  // Overwrite if exists
        audio_path.to_str().unwrap(),
    ])
    .output()?;

if !output.status.success() {
    anyhow::bail!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
}
```

### Step 2: Transcription

**Tool**: OpenAI Whisper CLI (Python)

**Command**:
```bash
whisper audio.wav \
        --language ja \           # Japanese language hint
        --model base \            # Model size
        --output_format txt \     # Plain text output
        --output_dir ./           # Output directory
```

**Environment**: Uses conda environment `GDA2025`

**Implementation**:
```rust
let python_path = "/home/yuc/miniconda3/envs/GDA2025/bin/python3";
let whisper_script = "/home/yuc/miniconda3/envs/GDA2025/bin/whisper";

let output = Command::new(python_path)
    .args(&[
        whisper_script,
        audio_path.to_str().unwrap(),
        "--language", "ja",
        "--model", &self.model,
        "--output_format", "txt",
        "--output_dir", transcript_dir.to_str().unwrap(),
    ])
    .output()?;
```

**Output**: Text file with transcribed Japanese text

### Step 3: Aggressive Cleanup

**Critical for disk space management**

```rust
// Step 3.1: Delete video file
if cleanup_config.delete_video_after_transcription {
    fs::remove_file(&video_path)?;
    queue.mark_video_deleted(job.id)?;

    info!(
        freed_mb = video_size / 1_000_000,
        "Deleted video file"
    );
}

// Step 3.2: Delete audio file
if cleanup_config.delete_audio_after_transcription {
    fs::remove_file(&audio_path)?;
    queue.mark_audio_deleted(job.id)?;

    info!(
        freed_mb = audio_size / 1_000_000,
        "Deleted audio file"
    );
}

// Step 3.3: Invalidate disk cache
disk_monitor.invalidate_cache();
```

**Timing**: Deletion happens immediately after transcription succeeds

**Benefits**:
- Frees ~500-2000 MB per episode (video + audio)
- Enables processing of 100,000+ episodes without massive storage
- Keeps only small text files (~30-50 KB per episode)

### Step 4: Database Update

```rust
// Update job with transcript information
queue.update_job_with_transcript(
    job.id,
    transcript_path,
    audio_size,
    transcript_size
)?;

// Update stage
queue.update_stage(job.id, JobStage::Transcribed)?;
```

## File Organization

### Audio Files (TEMPORARY)

**Location**: `data/audio/<mal_id>/`

**Format**: 16kHz mono WAV

**Lifecycle**: Created → Used → Deleted immediately

```
data/audio/
├── 5114/
│   ├── Fullmetal_Alchemist_Brotherhood_ep001.wav  # TEMPORARY
│   ├── Fullmetal_Alchemist_Brotherhood_ep002.wav  # TEMPORARY
│   └── ...
└── ...
```

### Transcript Files (PERMANENT)

**Location**: `data/transcripts/<mal_id>/`

**Format**: Plain UTF-8 text

**Size**: ~30-50 KB per episode

**Lifecycle**: Permanent (needed for tokenization)

```
data/transcripts/
├── 5114/
│   ├── ep001.txt                                   # PERMANENT
│   ├── ep002.txt                                   # PERMANENT
│   └── ...
└── ...
```

**Example content** (ep001.txt):
```
これは錬金術という科学の物語だ
等価交換という原則がある
何かを得るためには同等の代価が必要となる
それが錬金術の基本原理
...
```

### File Size Tracking

**Database preserves sizes even after deletion**:

```sql
-- jobs table tracks sizes for statistics
video_size_bytes INTEGER,      -- Preserved after deletion
audio_size_bytes INTEGER,      -- Preserved after deletion
transcript_size_bytes INTEGER, -- Current file size

-- Cleanup tracking
video_deleted BOOLEAN DEFAULT 0,
audio_deleted BOOLEAN DEFAULT 0,
```

**Benefits**:
- Can calculate total data processed
- Track storage savings from cleanup
- Statistics remain available for analysis

## Performance

### Transcription Speed

**Depends on model and hardware**:

| Model  | GPU (RTX 3090) | CPU (16-core) | Accuracy |
|--------|----------------|---------------|----------|
| tiny   | ~32x realtime  | ~8x realtime  | Lower    |
| base   | ~16x realtime  | ~4x realtime  | Good     |
| small  | ~6x realtime   | ~2x realtime  | Better   |
| medium | ~2x realtime   | ~1x realtime  | High     |
| large  | ~1x realtime   | ~0.5x realtime| Best     |

**Example** (base model, 24-minute episode):
- GPU: ~1.5 minutes transcription time
- CPU: ~6 minutes transcription time

### Throughput Estimation

**With 2 workers, base model, GPU**:
- ~80-120 episodes per day
- ~2,400-3,600 episodes per month

**With 4 workers, base model, GPU**:
- ~160-240 episodes per day
- ~4,800-7,200 episodes per month

### Resource Usage

- **CPU**: High during transcription (100% utilization)
- **GPU**: High if CUDA available (recommended)
- **Memory**: ~2-4 GB per worker
- **Disk I/O**: Moderate (reading video, writing small text)
- **Disk Space**: Minimal increase (transcripts are small)

### Optimization Tips

1. **Use GPU**: 4-8x faster than CPU
2. **Worker count**: Match to GPU/CPU cores
3. **Model selection**: base is good balance for Japanese
4. **Batch processing**: Let multiple workers run simultaneously

## Disk Space Impact

### Before Transcription

```
Videos: ~500 MB/episode × 50 episodes = 25 GB
Audio:  ~0 MB (not yet extracted)
Total:  25 GB
```

### During Transcription

```
Videos: 25 GB (still present)
Audio:  ~50 MB/episode × 4 concurrent = 200 MB
Total:  25.2 GB (peak)
```

### After Transcription (50 episodes)

```
Videos: 0 GB (deleted!)
Audio:  0 GB (deleted!)
Transcripts: ~40 KB/episode × 50 = 2 MB
Total:  2 MB
```

**Space freed**: ~25 GB → 2 MB = **99.99% reduction**

### Long-term Storage (172,066 episodes)

```
Transcripts: 40 KB × 172,066 = ~6.9 GB
```

**Compared to videos**: 172,066 × 500 MB = ~86 TB
**Savings**: 86 TB → 7 GB = **99.99% reduction**

## Error Handling

### Retry Strategy

**Automatic retry conditions**:
- FFmpeg extraction failed
- Whisper transcription failed
- File I/O errors

**Configuration**:
```sql
retry_count INTEGER DEFAULT 0,
max_retries INTEGER DEFAULT 3,
```

**Implementation**:
```rust
if job.retry_count < job.max_retries {
    // Reset to downloaded stage for retry
    queue.increment_retry(job.id)?;
    queue.update_stage(job.id, JobStage::Downloaded)?;
} else {
    // Mark as failed
    queue.update_stage_with_error(
        job.id,
        JobStage::Failed,
        format!("{:#}", e)
    )?;
}
```

### Common Errors

**1. Video File Not Found**
```
ERROR Video file not found: /path/to/video.mkv
```
**Cause**: Downloader didn't complete or file was manually deleted
**Solution**: Re-download the episode

**2. FFmpeg Extraction Failed**
```
ERROR FFmpeg failed: Invalid data found when processing input
```
**Cause**: Corrupted video file or unsupported format
**Solution**: Re-download, or skip if repeatedly fails

**3. Whisper Transcription Failed**
```
ERROR Whisper CLI failed: No speech detected
```
**Cause**: Silent video, audio track issues, or wrong language
**Solution**: Verify video has Japanese audio track

**4. Disk Full During Transcription**
```
ERROR Failed to write transcript: No space left on device
```
**Cause**: Root drive (SSD) full
**Solution**: Clean up logs, check data_dir space

### Hallucination Detection

**Issue**: Whisper sometimes hallucinates repeated text

**Example hallucination**:
```
ご視聴ありがとうございました
ご視聴ありがとうございました
ご視聴ありがとうございました
[repeated 50+ times]
```

**Detection** (future enhancement):
```rust
fn detect_hallucination(transcript: &str) -> bool {
    let lines: Vec<&str> = transcript.lines().collect();
    let total_lines = lines.len();

    // Check for repeated lines
    let unique_lines: HashSet<&str> = lines.iter().cloned().collect();
    let repetition_rate = 1.0 - (unique_lines.len() as f64 / total_lines as f64);

    // Flag if >80% repetition
    repetition_rate > 0.8
}
```

**Mitigation** (current):
- Manual review of suspicious transcripts
- Future: Automatic hallucination detection and filtering

## Integration with Other Components

### Upstream: anime-downloader

**Dependency**: Requires videos in `downloaded` stage

**Check if ready**:
```bash
sqlite3 data/jobs.db "
SELECT COUNT(*) FROM jobs WHERE stage = 'downloaded';
"
```

**Handoff**: Reads from `stage = 'downloaded'`

### Downstream: tokenizer (Phase 6)

**Output**: Plain text transcripts in `data/transcripts/`

**Handoff**: Updates job stage to `transcribed`

**Tokenizer pickup**: Reads transcript files, processes to tokens

## Monitoring and Statistics

### Real-time Monitoring

```bash
# Watch transcription progress
watch -n 5 'sqlite3 data/jobs.db "
SELECT stage, COUNT(*) as count
FROM jobs
WHERE stage IN (\"downloaded\", \"transcribing\", \"transcribed\")
GROUP BY stage
"'

# Check disk space freed
watch -n 30 'df -h /media/external/GDA2025'

# Monitor transcript sizes
watch -n 60 'du -sh data/transcripts/'
```

### Statistics Queries

```sql
-- Transcription success rate
SELECT
    COUNT(CASE WHEN stage = 'transcribed' THEN 1 END) as success,
    COUNT(CASE WHEN stage = 'failed' AND error_message LIKE '%transcrib%' THEN 1 END) as failed,
    ROUND(100.0 * COUNT(CASE WHEN stage = 'transcribed' THEN 1 END) /
          (COUNT(CASE WHEN stage = 'transcribed' THEN 1 END) +
           COUNT(CASE WHEN stage = 'failed' AND error_message LIKE '%transcrib%' THEN 1 END)), 2) as success_rate_pct
FROM jobs;

-- Average transcript size
SELECT
    AVG(transcript_size_bytes) / 1000 as avg_size_kb,
    MIN(transcript_size_bytes) / 1000 as min_size_kb,
    MAX(transcript_size_bytes) / 1000 as max_size_kb
FROM jobs
WHERE transcript_size_bytes IS NOT NULL;

-- Disk space freed by cleanup
SELECT
    SUM(video_size_bytes) / 1000000000 as video_deleted_gb,
    SUM(audio_size_bytes) / 1000000000 as audio_deleted_gb,
    SUM(video_size_bytes + audio_size_bytes) / 1000000000 as total_freed_gb
FROM jobs
WHERE video_deleted = 1 AND audio_deleted = 1;

-- Transcription progress by anime
SELECT
    anime_title,
    COUNT(*) as total_episodes,
    SUM(CASE WHEN stage = 'transcribed' THEN 1 ELSE 0 END) as transcribed,
    SUM(CASE WHEN stage = 'transcribing' THEN 1 ELSE 0 END) as in_progress
FROM jobs
GROUP BY anime_id, anime_title
HAVING transcribed > 0
ORDER BY transcribed DESC
LIMIT 20;
```

## Troubleshooting

### Issue: Slow Transcription

**Symptoms**: Much slower than expected transcription speed

**Diagnosis**:
- Check if GPU is being used: `nvidia-smi` (should show whisper process)
- Check CPU usage: `top` (should be near 100%)

**Solutions**:
- Install CUDA-enabled PyTorch in conda environment
- Use smaller model (base instead of large)
- Reduce worker count if system is overloaded

### Issue: High Failure Rate

**Symptoms**: Many transcription failures

**Diagnosis**:
```sql
SELECT error_message, COUNT(*) as count
FROM jobs
WHERE stage = 'failed' AND error_message LIKE '%transcrib%'
GROUP BY error_message
ORDER BY count DESC;
```

**Common causes**:
- Corrupted video files (re-download)
- Wrong audio language (anime has no Japanese audio)
- Whisper model not found (check installation)

### Issue: Hallucination Problems

**Symptoms**: Transcripts contain repeated phrases

**Diagnosis**: Manual review of transcript files

**Solutions**:
- Use larger model (base → small or medium)
- Future: Implement automatic hallucination detection

### Issue: Disk Space Not Freed

**Symptoms**: Storage usage doesn't decrease after transcription

**Diagnosis**:
```sql
SELECT video_deleted, audio_deleted, COUNT(*) as count
FROM jobs
WHERE stage = 'transcribed'
GROUP BY video_deleted, audio_deleted;
```

**Solutions**:
- Check cleanup config: `delete_video_after_transcription = true`
- Verify video files were actually deleted: `ls -lh /media/external/GDA2025/videos/*/episodes/`
- Check for failed deletions in logs

## Python Environment

### Conda Environment Setup

```bash
# Create environment
conda create -n GDA2025 python=3.10

# Activate
conda activate GDA2025

# Install Whisper
pip install openai-whisper

# Install dependencies
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu118

# Verify installation
whisper --help
```

### Environment Path

**Hardcoded in transcriber**:
```rust
let python_path = "/home/yuc/miniconda3/envs/GDA2025/bin/python3";
let whisper_script = "/home/yuc/miniconda3/envs/GDA2025/bin/whisper";
```

**Important**: Must use zsh, not bash (conda requirement)

## Future Enhancements

1. **Hallucination detection**: Automatic filtering of repeated text
2. **Timestamp preservation**: JSON format with segment timestamps
3. **Multi-language support**: Extend beyond Japanese
4. **Quality metrics**: Confidence scores from Whisper
5. **Alternative backends**: faster-whisper for better performance
6. **Batch processing**: Process multiple episodes in single Whisper call
7. **GPU optimization**: Better CUDA utilization

---

*Last updated: 2025-11-13*
*Status: Implemented and active (Phase 5)*
