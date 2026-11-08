use crate::db::models::SourceRow;
use crate::db::DbPool;
use crate::domain::{SourceConfig, SourceKind};
use crate::error::AppResult;

pub async fn upsert(pool: &DbPool, cfg: &SourceConfig) -> AppResult<()> {
    let row = SourceRow::from(cfg);
    sqlx::query(
        "INSERT INTO sources (kind, enabled, priority, display_name, extra_config, last_sync) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(kind) DO UPDATE SET enabled = excluded.enabled, \
         priority = excluded.priority, display_name = excluded.display_name, \
         extra_config = excluded.extra_config, last_sync = excluded.last_sync",
    )
    .bind(row.kind)
    .bind(row.enabled)
    .bind(row.priority)
    .bind(row.display_name)
    .bind(row.extra_config)
    .bind(row.last_sync)
    .execute(pool)
    .await?;
    Ok(())
}

async fn list_where(pool: &DbPool, only_enabled: bool) -> AppResult<Vec<SourceConfig>> {
    let sql = if only_enabled {
        "SELECT * FROM sources WHERE enabled = 1 ORDER BY priority, kind"
    } else {
        "SELECT * FROM sources ORDER BY priority, kind"
    };
    let rows = sqlx::query_as::<_, SourceRow>(sql).fetch_all(pool).await?;
    rows.into_iter().map(SourceConfig::try_from).collect()
}

pub async fn list(pool: &DbPool) -> AppResult<Vec<SourceConfig>> {
    list_where(pool, false).await
}

pub async fn list_enabled(pool: &DbPool) -> AppResult<Vec<SourceConfig>> {
    list_where(pool, true).await
}

pub async fn set_enabled(pool: &DbPool, kind: SourceKind, enabled: bool) -> AppResult<()> {
    sqlx::query("UPDATE sources SET enabled = ? WHERE kind = ?")
        .bind(i64::from(enabled))
        .bind(kind.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn touch_sync(pool: &DbPool, kind: SourceKind, ts: i64) -> AppResult<()> {
    sqlx::query("UPDATE sources SET last_sync = ? WHERE kind = ?")
        .bind(ts)
        .bind(kind.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Inserts the default row for every source that has none yet.
pub async fn seed_defaults(pool: &DbPool) -> AppResult<()> {
    for kind in SourceKind::ALL {
        let row = SourceRow::from(&SourceConfig::default_for(kind));
        sqlx::query(
            "INSERT OR IGNORE INTO sources (kind, enabled, priority, display_name, extra_config, last_sync) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(row.kind)
        .bind(row.enabled)
        .bind(row.priority)
        .bind(row.display_name)
        .bind(row.extra_config)
        .bind(row.last_sync)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::test_pool;

    #[tokio::test]
    async fn seed_is_idempotent_and_only_community_enabled() {
        let pool = test_pool().await;
        seed_defaults(&pool).await.unwrap();
        seed_defaults(&pool).await.unwrap();
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 4);
        let kinds: Vec<_> = all.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, SourceKind::ALL.to_vec());
        for c in &all {
            assert_eq!(c.enabled, c.kind == SourceKind::WingetCommunity);
        }
        let enabled = list_enabled(&pool).await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].kind, SourceKind::WingetCommunity);
    }

    #[tokio::test]
    async fn seed_keeps_existing_rows() {
        let pool = test_pool().await;
        let mut c = SourceConfig::default_for(SourceKind::Scoop);
        c.enabled = true;
        c.display_name = "Mine".into();
        upsert(&pool, &c).await.unwrap();
        seed_defaults(&pool).await.unwrap();
        let all = list(&pool).await.unwrap();
        let scoop = all.iter().find(|s| s.kind == SourceKind::Scoop).unwrap();
        assert_eq!(scoop, &c);
    }

    #[tokio::test]
    async fn set_enabled_and_touch_sync() {
        let pool = test_pool().await;
        seed_defaults(&pool).await.unwrap();
        set_enabled(&pool, SourceKind::MsStore, true).await.unwrap();
        set_enabled(&pool, SourceKind::WingetCommunity, false)
            .await
            .unwrap();
        touch_sync(&pool, SourceKind::MsStore, 1234).await.unwrap();
        let enabled = list_enabled(&pool).await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].kind, SourceKind::MsStore);
        assert_eq!(enabled[0].last_sync, Some(1234));
    }

    #[tokio::test]
    async fn upsert_round_trips_json_and_orders_by_priority() {
        let pool = test_pool().await;
        let mut a = SourceConfig::default_for(SourceKind::Chocolatey);
        a.priority = -1;
        a.extra_config = serde_json::json!({"url": "https://example.org"});
        let b = SourceConfig::default_for(SourceKind::WingetCommunity);
        upsert(&pool, &b).await.unwrap();
        upsert(&pool, &a).await.unwrap();
        upsert(&pool, &a).await.unwrap();
        let all = list(&pool).await.unwrap();
        assert_eq!(all, vec![a, b]);
    }
}
