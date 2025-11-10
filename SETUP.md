# Setup Guide

This guide covers the installation and setup process for the GDA2025 project.

## System Requirements

- **Operating System**: Linux (tested on Linux Mint with zsh)
- **Disk Space**: Minimum 250GB for data processing
- **Memory**: 8GB+ recommended (16GB+ for concurrent processing)
- **GPU**: Optional but highly recommended for Whisper transcription
  - CUDA-compatible GPU for faster processing
  - CPU-only mode is supported but significantly slower

## Dependencies

### 1. Rust Toolchain

Install Rust using rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

Verify installation:

```bash
rustc --version
cargo --version
```

### 2. FFmpeg

FFmpeg is required for audio extraction from video files.

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install ffmpeg
```

**Verify installation:**
```bash
ffmpeg -version
```

### 3. Python Environment (Miniconda)

#### Install Miniconda

Download and install Miniconda:

```bash
wget https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh
bash Miniconda3-latest-Linux-x86_64.sh
```

Follow the installation prompts. After installation, restart your shell or run:

```bash
source ~/.zshrc
```

#### Create Project Environment

Create a dedicated conda environment for the project:

```bash
conda create -n GDA2025 python=3.10
conda activate GDA2025
```

### 4. OpenAI Whisper

Install Whisper in the conda environment:

```bash
conda activate GDA2025
pip install openai-whisper
```

**For GPU support**, ensure you have PyTorch with CUDA installed:

```bash
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu118
```

**Verify installation:**

```bash
whisper --help
```

You should see the Whisper CLI help text with Japanese language support listed.

### 5. Animdl (Anime Downloader)

Install animdl in the same conda environment:

```bash
conda activate GDA2025
pip install animdl
```

**Verify installation:**

```bash
animdl --version
```

## Project Setup

### 1. Clone Repository

```bash
git clone <repository-url>
cd GDA2025
```

### 2. Build Project

Build all crates in the workspace:

```bash
cargo build --release
```

This will create optimized binaries in `target/release/`:
- `mal-scraper` - Scrapes anime metadata from MyAnimeList
- `anime-downloader` - Downloads anime episodes
- `transcriber` - Transcribes audio using Whisper

### 3. Configuration

Create a `config.toml` file in the project root (or use the provided default):

```toml
# Data directory for all files
data_dir = "data"

# Database path
database_path = "data/jobs.db"

# Log directory
log_dir = "data/logs"

[mal_scraper]
# MyAnimeList API rate limiting
requests_per_second = 2
max_retries = 3
cache_duration_days = 7

[disk_management]
# Disk space limits in GB
hard_limit_gb = 250
pause_threshold_gb = 230
resume_threshold_gb = 200

# Check interval in seconds
check_interval_seconds = 30
cache_duration_seconds = 5

# Concurrent workers
max_concurrent_downloads = 5
max_concurrent_transcriptions = 2

[disk_management.cleanup]
# Cleanup configuration
delete_video_after_transcription = true
delete_audio_after_transcription = true
delete_transcript_after_tokenization = false
delete_tokens_after_analysis = false
```

**Configuration Notes:**
- `hard_limit_gb`: Maximum disk space allowed (safety limit)
- `pause_threshold_gb`: Downloads pause when disk usage exceeds this
- `resume_threshold_gb`: Downloads resume when disk usage drops below this
- `max_concurrent_downloads`: Number of simultaneous video downloads
- `max_concurrent_transcriptions`: Number of simultaneous Whisper workers

### 4. Setup External Storage

**IMPORTANT**: To avoid excessive SSD wear from frequent video file writes/deletes, store data on an external drive or HDD.

#### Option A: Use External Storage (Recommended)

Mount your external storage and create the data directory:

```bash
# Replace with your actual mount point
EXTERNAL_STORAGE="/media/yuc/YOUR_EXTERNAL_DRIVE"

# Create data directory
sudo mkdir -p $EXTERNAL_STORAGE/GDA2025/data
sudo chown $USER:$USER $EXTERNAL_STORAGE/GDA2025/data

# Update config.toml to point to external storage
# Edit the [data] section:
# root_dir = "/media/yuc/YOUR_EXTERNAL_DRIVE/GDA2025/data"
```

#### Option B: Use Local Storage (Not Recommended for SSDs)

If using local storage, create the data directory in the project:

```bash
mkdir -p data/{videos,audio,transcripts,tokens,analysis,cache,logs}
```

**Note**: The default config.toml is configured for external storage. Adjust the `root_dir` path according to your setup.

### 5. Download Whisper Models

On first run, Whisper will automatically download the required model. To pre-download models:

```bash
conda activate GDA2025
whisper --model base --language ja /dev/null 2>/dev/null || true
```

Available models (in order of size/accuracy):
- `tiny` - Fastest, lowest accuracy
- `base` - Good balance (recommended for testing)
- `small` - Better accuracy
- `medium` - High accuracy
- `large` - Best accuracy, slowest

## Usage Workflow

### Step 1: Scrape Anime Metadata

Run the MAL scraper to populate the job queue:

```bash
RUST_LOG=info cargo run --release -p mal-scraper -- --config config.toml
```

This will:
- Fetch anime metadata from MyAnimeList API
- Cache results locally
- Populate the SQLite database with jobs

### Step 2: Download Episodes

Start the anime downloader:

```bash
RUST_LOG=info cargo run --release -p anime-downloader -- --config config.toml --workers 5
```

Options:
- `--workers N`: Number of concurrent download workers (default: 5)
- `--dry-run`: Test mode without actual downloads

The downloader will:
- Monitor disk space continuously
- Pause downloads when disk exceeds 230GB
- Resume when disk drops below 200GB

### Step 3: Transcribe Audio

Start the transcriber (can run concurrently with downloader):

```bash
conda activate GDA2025
RUST_LOG=info cargo run --release -p transcriber -- --config config.toml --workers 2 --model base
```

Options:
- `--workers N`: Number of concurrent transcription workers (default: 2)
- `--model NAME`: Whisper model to use (default: base)
- `--dry-run`: Test mode without actual transcription

The transcriber will:
- Extract audio from videos using FFmpeg
- Transcribe using Whisper
- Immediately delete video and audio files to free space
- Update job status in database

### Step 4: Monitor Progress

Check job queue statistics:

```bash
sqlite3 data/jobs.db "SELECT stage, COUNT(*) as count FROM jobs GROUP BY stage;"
```

Check disk usage:

```bash
du -sh data/*
```

## Troubleshooting

### Whisper CUDA Issues

If Whisper doesn't detect your GPU:

```bash
python -c "import torch; print(torch.cuda.is_available())"
```

If it returns `False`, reinstall PyTorch with CUDA support:

```bash
conda activate GDA2025
pip uninstall torch torchvision torchaudio
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu118
```

### FFmpeg Not Found

Ensure FFmpeg is in your PATH:

```bash
which ffmpeg
```

If not found, install it using your package manager.

### Disk Space Issues

If disk space monitoring isn't working correctly:

1. Check your `config.toml` thresholds
2. Verify the data directory path is correct
3. Check logs in `data/logs/` for errors

### Database Locked Errors

If you get "database is locked" errors:

1. Ensure only one instance of each binary is running
2. Check for stale lock files: `rm data/jobs.db-wal data/jobs.db-shm`
3. Restart the affected workers

## Performance Tips

1. **GPU Acceleration**: Use CUDA-enabled GPU for Whisper to speed up transcription by ~10×
2. **Concurrent Workers**: Adjust worker counts based on your hardware:
   - More download workers for faster network
   - More transcription workers for more GPUs
3. **Whisper Model Selection**: Use `base` or `small` for faster processing, `medium` or `large` for better accuracy
4. **Disk Thresholds**: Adjust thresholds based on your available disk space

## Next Steps

After setup is complete:
1. Run the MAL scraper to populate the job queue
2. Start the downloader and transcriber concurrently
3. Monitor progress through logs and database queries
4. Proceed to Phase 5: Tokenization (implementation pending)

## Logging

All components use structured logging with the `tracing` framework.

**Log Levels:**
- `ERROR`: Critical errors
- `WARN`: Warnings and retries
- `INFO`: Progress and statistics (default)
- `DEBUG`: Detailed debugging information

**Enable debug logging:**

```bash
RUST_LOG=debug cargo run --release -p transcriber
```

**Log files are saved to:**
- `data/logs/mal-scraper.log`
- `data/logs/anime-downloader.log`
- `data/logs/transcriber.log`
