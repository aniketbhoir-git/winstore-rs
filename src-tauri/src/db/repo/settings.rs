use std::collections::BTreeMap;

use crate::db::models::SettingRow;
use crate::db::DbPool;
use crate::error::AppResult;

pub async fn get(pool: &DbPool, key: &str) -> AppResult<Option<String>> {
    let v = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(v)
}

pub async fn set(pool: &DbPool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn all(pool: &DbPool) -> AppResult<BTreeMap<String, String>> {
    let rows = sqlx::query_as::<_, SettingRow>("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.key, r.value)).collect())
}

pub async fn delete(pool: &DbPool, key: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::test_pool;

    #[tokio::test]
    async fn get_set_overwrite_delete() {
        let pool = test_pool().await;
        assert_eq!(get(&pool, "theme").await.unwrap(), None);
        set(&pool, "theme", "dark").await.unwrap();
        assert_eq!(get(&pool, "theme").await.unwrap().as_deref(), Some("dark"));
        set(&pool, "theme", "light").await.unwrap();
        assert_eq!(get(&pool, "theme").await.unwrap().as_deref(), Some("light"));
        delete(&pool, "theme").await.unwrap();
        assert_eq!(get(&pool, "theme").await.unwrap(), None);
        delete(&pool, "theme").await.unwrap();
    }

    #[tokio::test]
    async fn all_returns_every_pair() {
        let pool = test_pool().await;
        assert!(all(&pool).await.unwrap().is_empty());
        set(&pool, "b", "2").await.unwrap();
        set(&pool, "a", "1").await.unwrap();
        let m = all(&pool).await.unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m["a"], "1");
        assert_eq!(m["b"], "2");
    }
}
