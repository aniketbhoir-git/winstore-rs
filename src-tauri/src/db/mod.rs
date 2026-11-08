use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::error::{AppError, AppResult};

pub type DbPool = sqlx::SqlitePool;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/db/migrations");

/// Opens (creating if needed) the on-disk database in WAL mode.
pub async fn init_pool(path: &Path) -> AppResult<DbPool> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// Single-connection in-memory pool, for tests.
pub async fn init_memory_pool() -> AppResult<DbPool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> AppResult<()> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|e| AppError::Sql(e.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn assert_migrated(pool: &DbPool) {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let t: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='settings'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(t, 1);
    }

    #[tokio::test]
    async fn migrates_memory_pool() {
        let pool = init_memory_pool().await.unwrap();
        run_migrations(&pool).await.unwrap();
        assert_migrated(&pool).await;
    }

    #[tokio::test]
    async fn migrates_file_pool_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(&dir.path().join("sub").join("store.db"))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        assert_migrated(&pool).await;
        pool.close().await;
    }
}
