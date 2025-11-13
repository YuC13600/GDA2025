//! Database operations for SQLite.
//!
//! This module handles all database connections, schema creation, and migrations.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use tracing::{debug, info};

/// Database connection wrapper
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let is_new = !path.exists();

        debug!(path = %path.display(), "Opening database");

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database at {}", path.display()))?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])
            .context("Failed to enable foreign keys")?;

        let mut db = Self { conn };

        if is_new {
            info!("Creating new database schema");
            db.create_schema()?;
        } else {
            debug!("Database already exists");
            // Run migrations for existing databases
            db.run_migrations()?;
        }

        Ok(db)
    }

    /// Create the database schema
    fn create_schema(&mut self) -> Result<()> {
        self.conn.execute_batch(include_str!("../schema.sql"))
            .context("Failed to create database schema")?;

        info!("Database schema created successfully");
        Ok(())
    }

    /// Get a reference to the underlying connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get a mutable reference to the underlying connection
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Check if a table exists
    pub fn table_exists(&self, table_name: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get the database version (from user_version pragma)
    pub fn get_version(&self) -> Result<i32> {
        let version: i32 = self.conn.query_row(
            "PRAGMA user_version",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Set the database version
    pub fn set_version(&self, version: i32) -> Result<()> {
        self.conn.execute(
            &format!("PRAGMA user_version = {}", version),
            [],
        )?;
        Ok(())
    }

    /// Run migrations for existing databases
    fn run_migrations(&mut self) -> Result<()> {
        // Check if anime_selection_cache table exists
        if !self.table_exists("anime_selection_cache")? {
            info!("Running migration: Creating anime_selection_cache table");
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS anime_selection_cache (
                    mal_id INTEGER PRIMARY KEY,
                    anime_title TEXT NOT NULL,
                    search_query TEXT NOT NULL,
                    selected_index INTEGER NOT NULL,
                    selected_title TEXT NOT NULL,
                    confidence TEXT NOT NULL CHECK(confidence IN ('high', 'medium', 'low', 'no_candidates')),
                    reason TEXT,
                    mal_episodes INTEGER,
                    selected_episodes INTEGER,
                    episode_match TEXT CHECK(episode_match IN ('exact', 'close', 'acceptable', 'mismatch', 'unknown', NULL)),
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (mal_id) REFERENCES anime(mal_id)
                );
                CREATE INDEX IF NOT EXISTS idx_selection_cache_confidence
                ON anime_selection_cache(confidence);
                CREATE INDEX IF NOT EXISTS idx_selection_cache_episode_match
                ON anime_selection_cache(episode_match);"
            ).context("Failed to create anime_selection_cache table")?;
            info!("Migration completed: anime_selection_cache table created");
        }

        Ok(())
    }

    /// Begin a transaction
    pub fn begin_transaction(&mut self) -> Result<rusqlite::Transaction<'_>> {
        self.conn.transaction()
            .context("Failed to begin transaction")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_database() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");

        let db = Database::open(&db_path)?;
        assert!(db_path.exists());

        // Check that tables were created
        assert!(db.table_exists("anime")?);
        assert!(db.table_exists("jobs")?);

        Ok(())
    }

    #[test]
    fn test_version() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");

        let db = Database::open(&db_path)?;

        let version = db.get_version()?;
        assert_eq!(version, 0);  // Default version

        db.set_version(1)?;
        assert_eq!(db.get_version()?, 1);

        Ok(())
    }
}
