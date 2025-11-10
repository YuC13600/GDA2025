//! Transcriber implementation.
//!
//! Transcribes audio from videos using Whisper and aggressively cleans up files.

use anyhow::{Context, Result};
use regex::Regex;
use shared::{CleanupConfig, DataPaths, DiskMonitor, Job, JobQueue, JobStage};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Transcriber worker.
pub struct Transcriber {
    /// Worker ID for logging
    worker_id: usize,
    /// Job queue
    queue: Arc<Mutex<JobQueue>>,
    /// Disk monitor
    disk_monitor: DiskMonitor,
    /// Data paths
    data_paths: DataPaths,
    /// Whisper model name
    model: String,
    /// Cleanup configuration
    cleanup_config: CleanupConfig,
    /// Dry run mode (don't actually transcribe)
    dry_run: bool,
    /// Number of completed transcriptions
    completed: usize,
    /// Number of failed transcriptions
    failed: usize,
}

impl Transcriber {
    /// Create a new transcriber worker.
    pub fn new(
        worker_id: usize,
        queue: Arc<Mutex<JobQueue>>,
        disk_monitor: DiskMonitor,
        data_paths: DataPaths,
        model: String,
        cleanup_config: CleanupConfig,
        dry_run: bool,
    ) -> Self {
        Self {
            worker_id,
            queue,
            disk_monitor,
            data_paths,
            model,
            cleanup_config,
            dry_run,
            completed: 0,
            failed: 0,
        }
    }

    /// Get worker ID.
    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    /// Run the transcription worker loop.
    pub async fn run(&mut self) -> Result<()> {
        info!(worker_id = self.worker_id, "Transcription worker started");

        loop {
            // Try to get next job from queue
            let job = match self.queue.lock().unwrap().dequeue_next(JobStage::Downloaded) {
                Ok(job) => job,
                Err(e) => {
                    // Check if error is "no jobs available"
                    let err_msg = format!("{}", e);
                    if err_msg.contains("No jobs available") {
                        debug!(worker_id = self.worker_id, "No more jobs in queue");
                        break;
                    }
                    return Err(e).context("Failed to dequeue job");
                }
            };

            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                anime_title = %job.anime_title,
                episode = job.episode,
                "Processing job"
            );

            // Update job stage to transcribing
            self.queue
                .lock()
                .unwrap()
                .update_stage(job.id, JobStage::Transcribing)
                .context("Failed to update job stage")?;

            // Process the job
            match self.process_job(&job).await {
                Ok((transcript_path, audio_size, transcript_size)) => {
                    info!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        audio_size_mb = audio_size / 1_000_000,
                        transcript_size_kb = transcript_size / 1_000,
                        "Transcription complete"
                    );

                    // Update job with transcript info
                    self.queue
                        .lock()
                        .unwrap()
                        .update_job_with_transcript(job.id, transcript_path, audio_size, transcript_size)
                        .context("Failed to update job with transcript info")?;

                    // Update stage to transcribed
                    self.queue
                        .lock()
                        .unwrap()
                        .update_stage(job.id, JobStage::Transcribed)
                        .context("Failed to update job stage")?;

                    self.completed += 1;

                    // Invalidate disk cache to reflect deleted files
                    self.disk_monitor.invalidate_cache();
                }
                Err(e) => {
                    error!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        error = %e,
                        "Transcription failed"
                    );

                    // Check if we should retry
                    if job.retry_count < job.max_retries {
                        warn!(
                            job_id = job.id,
                            retry_count = job.retry_count + 1,
                            max_retries = job.max_retries,
                            "Retrying job"
                        );

                        // Increment retry count and reset to downloaded
                        self.queue
                            .lock()
                            .unwrap()
                            .increment_retry(job.id)
                            .context("Failed to increment retry count")?;
                        self.queue
                            .lock()
                            .unwrap()
                            .update_stage(job.id, JobStage::Downloaded)
                            .context("Failed to reset job stage")?;
                    } else {
                        error!(
                            job_id = job.id,
                            "Max retries exceeded, marking job as failed"
                        );

                        // Mark as failed
                        self.queue
                            .lock()
                            .unwrap()
                            .update_stage_with_error(job.id, JobStage::Failed, format!("{:#}", e))
                            .context("Failed to update job as failed")?;

                        self.failed += 1;
                    }
                }
            }

            // Small delay between jobs
            sleep(std::time::Duration::from_millis(100)).await;
        }

        info!(
            worker_id = self.worker_id,
            completed = self.completed,
            failed = self.failed,
            "Transcription worker finished"
        );

        Ok(())
    }

    /// Process a single job: extract audio, transcribe, cleanup.
    ///
    /// Returns: (transcript_path, audio_size, transcript_size)
    async fn process_job(&self, job: &Job) -> Result<(PathBuf, u64, u64)> {
        // Get video path from job
        let video_path = job
            .video_path
            .as_ref()
            .context("Job has no video path")?;
        let video_path = PathBuf::from(video_path);

        if !video_path.exists() {
            anyhow::bail!("Video file not found: {}", video_path.display());
        }

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            video_path = %video_path.display(),
            "Starting transcription process"
        );

        // Step 1: Extract audio
        let audio_path = self.extract_audio(&video_path, job).await?;
        let audio_size = fs::metadata(&audio_path)?.len();

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            audio_size_mb = audio_size / 1_000_000,
            "Audio extracted"
        );

        // Step 2: Transcribe
        let transcript_path = self.transcribe(&audio_path, job).await?;
        let transcript_size = fs::metadata(&transcript_path)?.len();

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            transcript_size_kb = transcript_size / 1_000,
            "Transcription complete"
        );

        // Step 3: AGGRESSIVE CLEANUP - Delete video and audio immediately
        if self.cleanup_config.delete_video_after_transcription {
            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                video_path = %video_path.display(),
                "Deleting video file"
            );
            fs::remove_file(&video_path)
                .with_context(|| format!("Failed to delete video: {}", video_path.display()))?;

            // Mark video as deleted in database
            self.queue
                .lock()
                .unwrap()
                .mark_video_deleted(job.id)
                .context("Failed to mark video as deleted")?;
        }

        if self.cleanup_config.delete_audio_after_transcription {
            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                audio_path = %audio_path.display(),
                "Deleting audio file"
            );
            fs::remove_file(&audio_path)
                .with_context(|| format!("Failed to delete audio: {}", audio_path.display()))?;

            // Mark audio as deleted in database
            self.queue
                .lock()
                .unwrap()
                .mark_audio_deleted(job.id)
                .context("Failed to mark audio as deleted")?;
        }

        let video_size = job.video_size_bytes.unwrap_or(0);
        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            freed_mb = (video_size + audio_size) / 1_000_000,
            "Freed disk space by deleting video and audio"
        );

        Ok((transcript_path, audio_size, transcript_size))
    }

    /// Extract audio from video using FFmpeg.
    ///
    /// Converts to 16kHz mono WAV format for Whisper.
    async fn extract_audio(&self, video_path: &PathBuf, job: &Job) -> Result<PathBuf> {
        let audio_dir = self.data_paths.audio_dir(job.mal_id);
        fs::create_dir_all(&audio_dir)?;

        let safe_title = sanitize_filename(&job.anime_title);
        let filename = format!("{}_ep{:03}.wav", safe_title, job.episode);
        let audio_path = audio_dir.join(&filename);

        // Check if already extracted
        if audio_path.exists() {
            warn!(
                job_id = job.id,
                path = %audio_path.display(),
                "Audio file already exists, skipping extraction"
            );
            return Ok(audio_path);
        }

        if self.dry_run {
            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                "Dry run: would extract audio from {}",
                video_path.display()
            );
            // Create empty file for testing
            fs::write(&audio_path, b"")?;
            return Ok(audio_path);
        }

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            video = %video_path.display(),
            audio = %audio_path.display(),
            "Extracting audio with FFmpeg"
        );

        // Use FFmpeg to extract audio
        // ffmpeg -i input.mp4 -vn -acodec pcm_s16le -ar 16000 -ac 1 output.wav
        let status = Command::new("ffmpeg")
            .arg("-i")
            .arg(video_path)
            .arg("-vn") // No video
            .arg("-acodec")
            .arg("pcm_s16le") // 16-bit PCM
            .arg("-ar")
            .arg("16000") // 16kHz sample rate
            .arg("-ac")
            .arg("1") // Mono
            .arg("-y") // Overwrite output file
            .arg(&audio_path)
            .status()
            .context("Failed to execute ffmpeg command")?;

        if !status.success() {
            anyhow::bail!(
                "ffmpeg failed with exit code: {:?}",
                status.code().unwrap_or(-1)
            );
        }

        // Verify file was created
        if !audio_path.exists() {
            anyhow::bail!("Audio file was not created: {}", audio_path.display());
        }

        Ok(audio_path)
    }

    /// Transcribe audio using Whisper.
    ///
    /// Uses the whisper CLI (from openai-whisper Python package).
    async fn transcribe(&self, audio_path: &PathBuf, job: &Job) -> Result<PathBuf> {
        let transcript_dir = self.data_paths.transcript_dir(job.mal_id);
        fs::create_dir_all(&transcript_dir)?;

        let safe_title = sanitize_filename(&job.anime_title);
        let filename = format!("{}_ep{:03}.txt", safe_title, job.episode);
        let transcript_path = transcript_dir.join(&filename);

        // Check if already transcribed
        if transcript_path.exists() {
            warn!(
                job_id = job.id,
                path = %transcript_path.display(),
                "Transcript already exists, skipping transcription"
            );
            return Ok(transcript_path);
        }

        if self.dry_run {
            info!(
                worker_id = self.worker_id,
                job_id = job.id,
                "Dry run: would transcribe {}",
                audio_path.display()
            );
            // Create dummy transcript for testing
            fs::write(&transcript_path, "Dry run transcript")?;
            return Ok(transcript_path);
        }

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            audio = %audio_path.display(),
            model = %self.model,
            "Transcribing with Whisper"
        );

        // Use whisper CLI
        // whisper audio.wav --model base --language ja --output_dir /path/to/dir --output_format txt
        let status = Command::new("whisper")
            .arg(audio_path)
            .arg("--model")
            .arg(&self.model)
            .arg("--language")
            .arg("ja") // Japanese
            .arg("--output_dir")
            .arg(&transcript_dir)
            .arg("--output_format")
            .arg("txt")
            .arg("--verbose")
            .arg("False") // Less noise in logs
            .status()
            .context("Failed to execute whisper command")?;

        if !status.success() {
            anyhow::bail!(
                "whisper failed with exit code: {:?}",
                status.code().unwrap_or(-1)
            );
        }

        // Whisper creates output with different naming: <audio_stem>.txt
        let audio_stem = audio_path.file_stem().unwrap().to_string_lossy();
        let whisper_output = transcript_dir.join(format!("{}.txt", audio_stem));

        // Rename to our expected format if needed
        if whisper_output != transcript_path && whisper_output.exists() {
            fs::rename(&whisper_output, &transcript_path)?;
        }

        // Verify file was created
        if !transcript_path.exists() {
            anyhow::bail!(
                "Transcript file was not created: {}",
                transcript_path.display()
            );
        }

        // Post-process: detect and remove hallucinations
        self.clean_transcript(&transcript_path)?;

        Ok(transcript_path)
    }

    /// Clean transcript by removing hallucination patterns.
    fn clean_transcript(&self, transcript_path: &PathBuf) -> Result<()> {
        let content = fs::read_to_string(transcript_path)?;

        // Detect common hallucination patterns
        let hallucination_patterns = vec![
            Regex::new(r"(?i)thank you for watching").unwrap(),
            Regex::new(r"(?i)please subscribe").unwrap(),
            Regex::new(r"(?i)like and subscribe").unwrap(),
            // Repeated segments (same line appearing 3+ times)
            // This is simplified; a more robust implementation would use edit distance
        ];

        let mut lines: Vec<&str> = content.lines().collect();

        // Remove lines matching hallucination patterns
        lines.retain(|line| {
            for pattern in &hallucination_patterns {
                if pattern.is_match(line) {
                    warn!("Removed hallucination: {}", line);
                    return false;
                }
            }
            true
        });

        // Remove consecutive duplicate lines
        lines.dedup();

        let cleaned_content = lines.join("\n");

        // Write back if modified
        if cleaned_content != content {
            fs::write(transcript_path, cleaned_content)?;
            info!("Cleaned transcript: {}", transcript_path.display());
        }

        Ok(())
    }
}

/// Sanitize filename by removing/replacing invalid characters.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(
            sanitize_filename("Fullmetal Alchemist: Brotherhood"),
            "Fullmetal Alchemist_ Brotherhood"
        );
    }
}
