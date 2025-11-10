//! Job queue management for the processing pipeline.
//!
//! This module provides a high-level API for managing jobs in the SQLite database,
//! including creating jobs, updating status, and deduplication.

use crate::models::*;
use crate::Database;
use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use tracing::{debug, info, warn};

/// Job queue manager
pub struct JobQueue {
    db: Database,
}

impl JobQueue {
    /// Create a new job queue with the given database
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Get or create an anime entry (deduplication)
    ///
    /// If an anime with the given MAL ID already exists, return its database ID.
    /// Otherwise, insert the new anime and return the new ID.
    pub fn get_or_create_anime(&mut self, anime: &Anime) -> Result<i64> {
        let conn = self.db.conn_mut();

        // Try to find existing anime by MAL ID
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM anime WHERE mal_id = ?1",
                params![anime.mal_id],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to query for existing anime")?;

        if let Some(id) = existing_id {
            debug!(mal_id = anime.mal_id, db_id = id, "Anime already exists");
            return Ok(id);
        }

        // Insert new anime
        conn.execute(
            "INSERT INTO anime (
                mal_id, title, title_english, title_japanese, title_synonyms,
                type, episodes_total, status,
                aired_from, aired_to, season, year,
                genres, explicit_genres, themes, demographics, studios,
                score, scored_by, rank, popularity,
                source, rating, duration_minutes,
                processing_status, fetched_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17,
                ?18, ?19, ?20, ?21,
                ?22, ?23, ?24,
                ?25, ?26, ?27
            )",
            params![
                anime.mal_id,
                anime.title,
                anime.title_english,
                anime.title_japanese,
                serde_json::to_string(&anime.title_synonyms)?,
                anime.anime_type,
                anime.episodes_total,
                anime.status,
                anime.aired_from,
                anime.aired_to,
                anime.season,
                anime.year,
                serde_json::to_string(&anime.genres)?,
                serde_json::to_string(&anime.explicit_genres)?,
                serde_json::to_string(&anime.themes)?,
                serde_json::to_string(&anime.demographics)?,
                serde_json::to_string(&anime.studios)?,
                anime.score,
                anime.scored_by,
                anime.rank,
                anime.popularity,
                anime.source,
                anime.rating,
                anime.duration_minutes,
                anime.processing_status.to_string(),
                anime.fetched_at,
                anime.updated_at,
            ],
        )
        .context("Failed to insert anime")?;

        let id = conn.last_insert_rowid();
        info!(mal_id = anime.mal_id, db_id = id, title = %anime.title, "Created new anime entry");

        Ok(id)
    }

    /// Enqueue a new job (with deduplication)
    ///
    /// If a job for the same anime/episode already exists, return the existing job ID.
    /// Otherwise, create a new job and return its ID.
    pub fn enqueue(&mut self, job: &NewJob) -> Result<i64> {
        let conn = self.db.conn_mut();

        // Try to insert, handle UNIQUE constraint violation
        match conn.execute(
            "INSERT INTO jobs (anime_id, mal_id, anime_title, episode, stage, priority)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5)",
            params![
                job.anime_id,
                job.mal_id,
                job.anime_title,
                job.episode,
                job.priority,
            ],
        ) {
            Ok(_) => {
                let id = conn.last_insert_rowid();
                debug!(
                    job_id = id,
                    anime_id = job.anime_id,
                    episode = job.episode,
                    "Enqueued new job"
                );
                Ok(id)
            }
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                // Job already exists, return existing ID
                let existing_id: i64 = conn.query_row(
                    "SELECT id FROM jobs WHERE anime_id = ?1 AND episode = ?2",
                    params![job.anime_id, job.episode],
                    |row| row.get(0),
                )?;

                debug!(
                    job_id = existing_id,
                    anime_id = job.anime_id,
                    episode = job.episode,
                    "Job already exists"
                );

                Ok(existing_id)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Dequeue the next job for a specific stage (atomic operation)
    ///
    /// This atomically moves a job from `from_stage` to `to_stage` and returns it.
    /// If no jobs are available, returns None.
    pub fn dequeue(&mut self, from_stage: JobStage, to_stage: JobStage) -> Result<Option<Job>> {
        let conn = self.db.conn_mut();

        // Start a transaction for atomicity
        let tx = conn.transaction()?;

        // Find and update the next job
        let updated = tx.execute(
            "UPDATE jobs SET stage = ?1, started_at = CURRENT_TIMESTAMP
             WHERE id = (
                 SELECT id FROM jobs
                 WHERE stage = ?2
                 ORDER BY priority DESC, created_at ASC
                 LIMIT 1
             )",
            params![to_stage.to_string(), from_stage.to_string()],
        )?;

        if updated == 0 {
            // No jobs available
            tx.commit()?;
            return Ok(None);
        }

        // Fetch the job we just updated
        let job = tx.query_row(
            "SELECT * FROM jobs WHERE stage = ?1 ORDER BY updated_at DESC LIMIT 1",
            params![to_stage.to_string()],
            row_to_job,
        )?;

        tx.commit()?;

        debug!(
            job_id = job.id,
            from_stage = %from_stage,
            to_stage = %to_stage,
            "Dequeued job"
        );

        Ok(Some(job))
    }

    /// Update job progress and optionally change stage
    pub fn update_progress(&mut self, job_id: i64, progress: f64, stage: Option<JobStage>) -> Result<()> {
        let conn = self.db.conn_mut();

        if let Some(new_stage) = stage {
            conn.execute(
                "UPDATE jobs SET progress = ?1, stage = ?2 WHERE id = ?3",
                params![progress, new_stage.to_string(), job_id],
            )?;

            debug!(
                job_id = job_id,
                progress = %format!("{:.1}%", progress * 100.0),
                stage = %new_stage,
                "Updated job progress and stage"
            );
        } else {
            conn.execute(
                "UPDATE jobs SET progress = ?1 WHERE id = ?2",
                params![progress, job_id],
            )?;

            debug!(
                job_id = job_id,
                progress = %format!("{:.1}%", progress * 100.0),
                "Updated job progress"
            );
        }

        Ok(())
    }

    /// Update job metadata (file sizes, counts, paths)
    pub fn update_metadata(&mut self, job_id: i64, metadata: &JobMetadata) -> Result<()> {
        let conn = self.db.conn_mut();

        let mut updates = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(size) = metadata.video_size_bytes {
            updates.push("video_size_bytes = ?");
            params_vec.push(Box::new(size as i64));
        }
        if let Some(size) = metadata.audio_size_bytes {
            updates.push("audio_size_bytes = ?");
            params_vec.push(Box::new(size as i64));
        }
        if let Some(size) = metadata.transcript_size_bytes {
            updates.push("transcript_size_bytes = ?");
            params_vec.push(Box::new(size as i64));
        }
        if let Some(size) = metadata.tokens_size_bytes {
            updates.push("tokens_size_bytes = ?");
            params_vec.push(Box::new(size as i64));
        }
        if let Some(duration) = metadata.duration_seconds {
            updates.push("duration_seconds = ?");
            params_vec.push(Box::new(duration as i64));
        }
        if let Some(count) = metadata.word_count {
            updates.push("word_count = ?");
            params_vec.push(Box::new(count as i64));
        }
        if let Some(count) = metadata.token_count {
            updates.push("token_count = ?");
            params_vec.push(Box::new(count as i64));
        }
        if let Some(ref path) = metadata.video_path {
            updates.push("video_path = ?");
            params_vec.push(Box::new(path.clone()));
        }
        if let Some(ref path) = metadata.transcript_path {
            updates.push("transcript_path = ?");
            params_vec.push(Box::new(path.clone()));
        }
        if let Some(ref path) = metadata.tokens_path {
            updates.push("tokens_path = ?");
            params_vec.push(Box::new(path.clone()));
        }

        if updates.is_empty() {
            return Ok(());
        }

        params_vec.push(Box::new(job_id));

        let sql = format!("UPDATE jobs SET {} WHERE id = ?", updates.join(", "));
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        conn.execute(&sql, params_refs.as_slice())?;

        debug!(job_id = job_id, "Updated job metadata");

        Ok(())
    }

    /// Mark a file as deleted
    pub fn mark_file_deleted(&mut self, job_id: i64, file_type: FileType) -> Result<()> {
        let conn = self.db.conn_mut();

        let column = match file_type {
            FileType::Video => "video_deleted",
            FileType::Audio => "audio_deleted",
        };

        let sql = format!("UPDATE jobs SET {} = 1 WHERE id = ?", column);
        conn.execute(&sql, params![job_id])?;

        debug!(job_id = job_id, file_type = ?file_type, "Marked file as deleted");

        Ok(())
    }

    /// Mark a job as failed with error message
    pub fn fail_job(&mut self, job_id: i64, error: &str) -> Result<()> {
        let conn = self.db.conn_mut();

        conn.execute(
            "UPDATE jobs
             SET stage = 'failed',
                 error_message = ?1,
                 retry_count = retry_count + 1
             WHERE id = ?2",
            params![error, job_id],
        )?;

        warn!(job_id = job_id, error = %error, "Job failed");

        Ok(())
    }

    /// Retry all failed jobs (reset to queued if under max_retries)
    pub fn retry_failed(&mut self) -> Result<usize> {
        let conn = self.db.conn_mut();

        let updated = conn.execute(
            "UPDATE jobs
             SET stage = 'queued',
                 error_message = NULL,
                 progress = 0.0
             WHERE stage = 'failed' AND retry_count < max_retries",
            [],
        )?;

        if updated > 0 {
            info!(count = updated, "Retrying failed jobs");
        }

        Ok(updated)
    }

    /// Get all jobs (for TUI display)
    pub fn get_all_jobs(&self) -> Result<Vec<Job>> {
        let conn = self.db.conn();

        let mut stmt = conn.prepare(
            "SELECT * FROM jobs ORDER BY priority DESC, created_at ASC"
        )?;

        let jobs = stmt
            .query_map([], row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Get jobs by stage
    pub fn get_jobs_by_stage(&self, stage: JobStage) -> Result<Vec<Job>> {
        let conn = self.db.conn();

        let mut stmt = conn.prepare(
            "SELECT * FROM jobs WHERE stage = ?1 ORDER BY priority DESC, created_at ASC"
        )?;

        let jobs = stmt
            .query_map(params![stage.to_string()], row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Get job statistics
    pub fn get_stats(&self) -> Result<JobStats> {
        let conn = self.db.conn();

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))?;

        let mut stmt = conn.prepare("SELECT stage, COUNT(*) FROM jobs GROUP BY stage")?;
        let mut stage_counts = std::collections::HashMap::new();

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (stage, count) = row?;
            stage_counts.insert(stage, count);
        }

        Ok(JobStats {
            total: total as usize,
            queued: *stage_counts.get("queued").unwrap_or(&0) as usize,
            downloading: *stage_counts.get("downloading").unwrap_or(&0) as usize,
            downloaded: *stage_counts.get("downloaded").unwrap_or(&0) as usize,
            transcribing: *stage_counts.get("transcribing").unwrap_or(&0) as usize,
            transcribed: *stage_counts.get("transcribed").unwrap_or(&0) as usize,
            tokenizing: *stage_counts.get("tokenizing").unwrap_or(&0) as usize,
            tokenized: *stage_counts.get("tokenized").unwrap_or(&0) as usize,
            analyzing: *stage_counts.get("analyzing").unwrap_or(&0) as usize,
            complete: *stage_counts.get("complete").unwrap_or(&0) as usize,
            failed: *stage_counts.get("failed").unwrap_or(&0) as usize,
        })
    }

}

/// Helper: Convert a database row to a Job
fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
        Ok(Job {
            id: row.get(0)?,
            anime_id: row.get(1)?,
            anime_title: row.get(2)?,
            anime_title_english: row.get(3)?,
            mal_id: row.get::<_, Option<i64>>(4)?.map(|x| x as u32).unwrap_or(0),
            episode: row.get::<_, i64>(5)? as u32,
            season: row.get::<_, Option<i64>>(6)?.map(|x| x as i32),
            year: row.get::<_, Option<i64>>(7)?.map(|x| x as i32),
            stage: row.get::<_, String>(8)?.parse().unwrap_or(JobStage::Queued),
            progress: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
            started_at: row.get(12)?,
            completed_at: row.get(13)?,
            error_message: row.get(14)?,
            retry_count: row.get::<_, i64>(15)? as u32,
            max_retries: row.get::<_, i64>(16)? as u32,
            video_path: row.get(17)?,
            transcript_path: row.get(18)?,
            tokens_path: row.get(19)?,
            analysis_path: row.get(20)?,
            duration_seconds: row.get::<_, Option<i64>>(21)?.map(|x| x as u32),
            video_size_bytes: row.get::<_, Option<i64>>(22)?.map(|x| x as u64),
            audio_size_bytes: row.get::<_, Option<i64>>(23)?.map(|x| x as u64),
            transcript_size_bytes: row.get::<_, Option<i64>>(24)?.map(|x| x as u64),
            tokens_size_bytes: row.get::<_, Option<i64>>(25)?.map(|x| x as u64),
            word_count: row.get::<_, Option<i64>>(26)?.map(|x| x as u32),
            token_count: row.get::<_, Option<i64>>(27)?.map(|x| x as u32),
            video_deleted: row.get(28)?,
            audio_deleted: row.get(29)?,
            priority: row.get::<_, i64>(30)? as i32,
            depends_on: row.get::<_, Option<i64>>(31)?,
        })
}

/// Job statistics
#[derive(Debug, Clone)]
pub struct JobStats {
    pub total: usize,
    pub queued: usize,
    pub downloading: usize,
    pub downloaded: usize,
    pub transcribing: usize,
    pub transcribed: usize,
    pub tokenizing: usize,
    pub tokenized: usize,
    pub analyzing: usize,
    pub complete: usize,
    pub failed: usize,
}
