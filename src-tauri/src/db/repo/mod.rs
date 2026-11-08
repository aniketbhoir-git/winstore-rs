//! Async repository functions, one module per table. No business logic.

pub mod installs;
pub mod operation_log;
pub mod packages;
pub mod settings;
pub mod sources;

#[cfg(test)]
pub(crate) async fn test_pool() -> crate::db::DbPool {
    let pool = crate::db::init_memory_pool().await.unwrap();
    crate::db::run_migrations(&pool).await.unwrap();
    pool
}
