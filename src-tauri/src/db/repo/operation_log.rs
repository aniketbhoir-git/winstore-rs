use crate::db::models::OperationLogRow;
use crate::db::DbPool;
use crate::domain::{OperationLogEntry, PackageId};
use crate::error::AppResult;

/// Appends an entry (any `id` on the input is ignored) and returns the new row id.
pub async fn append(pool: &DbPool, entry: &OperationLogEntry) -> AppResult<i64> {
    let row = OperationLogRow::from(entry);
    let res = sqlx::query(
        "INSERT INTO operation_log (ts, level, operation, package_id, message, detail) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(row.ts)
    .bind(row.level)
    .bind(row.operation)
    .bind(row.package_id)
    .bind(row.message)
    .bind(row.detail)
    .execute(pool)
    .await?;
    Ok(res.last_insert_rowid())
}

/// Newest entries first.
pub async fn recent(pool: &DbPool, limit: u32) -> AppResult<Vec<OperationLogEntry>> {
    let rows = sqlx::query_as::<_, OperationLogRow>(
        "SELECT * FROM operation_log ORDER BY ts DESC, id DESC LIMIT ?",
    )
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(OperationLogEntry::try_from).collect()
}

pub async fn for_package(
    pool: &DbPool,
    id: &PackageId,
    limit: u32,
) -> AppResult<Vec<OperationLogEntry>> {
    let rows = sqlx::query_as::<_, OperationLogRow>(
        "SELECT * FROM operation_log WHERE package_id = ? ORDER BY ts DESC, id DESC LIMIT ?",
    )
    .bind(id.as_str())
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(OperationLogEntry::try_from).collect()
}

/// Deletes entries with `ts < cutoff_ts`; returns the number removed.
pub async fn prune_older_than(pool: &DbPool, cutoff_ts: i64) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM operation_log WHERE ts < ?")
        .bind(cutoff_ts)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::test_pool;
    use crate::domain::{LogLevel, Operation, SourceKind};

    fn entry(ts: i64, pkg: Option<&str>, msg: &str) -> OperationLogEntry {
        OperationLogEntry {
            id: None,
            ts,
            level: LogLevel::Info,
            operation: Some(Operation::Install),
            package_id: pkg.map(|p| PackageId::new(SourceKind::Scoop, p)),
            message: msg.into(),
            detail: Some("d".into()),
        }
    }

    #[tokio::test]
    async fn append_returns_increasing_ids() {
        let pool = test_pool().await;
        let a = append(&pool, &entry(1, None, "a")).await.unwrap();
        let b = append(&pool, &entry(2, None, "b")).await.unwrap();
        assert!(b > a);
    }

    #[tokio::test]
    async fn recent_is_newest_first_and_limited() {
        let pool = test_pool().await;
        append(&pool, &entry(10, None, "old")).await.unwrap();
        append(&pool, &entry(30, None, "new")).await.unwrap();
        append(&pool, &entry(20, None, "mid")).await.unwrap();
        let r = recent(&pool, 10).await.unwrap();
        let msgs: Vec<_> = r.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs, ["new", "mid", "old"]);
        assert!(r.iter().all(|e| e.id.is_some()));
        assert_eq!(recent(&pool, 2).await.unwrap().len(), 2);
        let first = &r[0];
        assert_eq!(first.level, LogLevel::Info);
        assert_eq!(first.operation, Some(Operation::Install));
        assert_eq!(first.detail.as_deref(), Some("d"));
    }

    #[tokio::test]
    async fn for_package_filters() {
        let pool = test_pool().await;
        append(&pool, &entry(1, Some("a"), "a1")).await.unwrap();
        append(&pool, &entry(2, Some("b"), "b1")).await.unwrap();
        append(&pool, &entry(3, Some("a"), "a2")).await.unwrap();
        let id = PackageId::new(SourceKind::Scoop, "a");
        let r = for_package(&pool, &id, 10).await.unwrap();
        let msgs: Vec<_> = r.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs, ["a2", "a1"]);
        assert_eq!(for_package(&pool, &id, 1).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn prune_removes_old_only() {
        let pool = test_pool().await;
        append(&pool, &entry(1, None, "x")).await.unwrap();
        append(&pool, &entry(5, None, "y")).await.unwrap();
        append(&pool, &entry(10, None, "z")).await.unwrap();
        assert_eq!(prune_older_than(&pool, 5).await.unwrap(), 1);
        assert_eq!(recent(&pool, 10).await.unwrap().len(), 2);
        assert_eq!(prune_older_than(&pool, 5).await.unwrap(), 0);
    }
}
