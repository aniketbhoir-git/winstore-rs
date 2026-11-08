use crate::db::models::PackageRow;
use crate::db::DbPool;
use crate::domain::{PackageDetail, PackageSummary, SourceKind};
use crate::error::AppResult;

const UPSERT_SUMMARY: &str = "INSERT INTO packages (source, package_id, name, publisher, \
    latest_version, description, icon_url, rating, rating_count, is_paid, packaging, \
    detail_json, fetched_at, detail_fetched_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
    ON CONFLICT(source, package_id) DO UPDATE SET name = excluded.name, \
    publisher = excluded.publisher, latest_version = excluded.latest_version, \
    description = excluded.description, icon_url = excluded.icon_url, \
    rating = excluded.rating, rating_count = excluded.rating_count, \
    is_paid = excluded.is_paid, packaging = excluded.packaging, \
    fetched_at = excluded.fetched_at";

/// `detail_fetched_at` is `Some` only for a detail write; a summary write keeps the old one.
async fn write_row(
    pool: &DbPool,
    row: PackageRow,
    detail_fetched_at: Option<i64>,
) -> AppResult<()> {
    let sql = if detail_fetched_at.is_some() {
        format!(
            "{UPSERT_SUMMARY}, detail_json = excluded.detail_json, \
             detail_fetched_at = excluded.detail_fetched_at"
        )
    } else {
        UPSERT_SUMMARY.to_string()
    };
    sqlx::query(&sql)
        .bind(row.source)
        .bind(row.package_id)
        .bind(row.name)
        .bind(row.publisher)
        .bind(row.latest_version)
        .bind(row.description)
        .bind(row.icon_url)
        .bind(row.rating)
        .bind(row.rating_count)
        .bind(row.is_paid)
        .bind(row.packaging)
        .bind(row.detail_json)
        .bind(row.fetched_at)
        .bind(detail_fetched_at)
        .execute(pool)
        .await?;
    Ok(())
}

/// Inserts or refreshes the summary columns; a previously cached detail is kept.
pub async fn upsert_summary(
    pool: &DbPool,
    summary: &PackageSummary,
    fetched_at: i64,
) -> AppResult<()> {
    let row = PackageRow::from_summary(summary, None, fetched_at)?;
    write_row(pool, row, None).await
}

/// Stores the full detail and refreshes the summary columns from it.
pub async fn upsert_detail(
    pool: &DbPool,
    detail: &PackageDetail,
    fetched_at: i64,
) -> AppResult<()> {
    let row = PackageRow::from_summary(&detail.summary, Some(detail), fetched_at)?;
    write_row(pool, row, Some(fetched_at)).await
}

async fn get_row(
    pool: &DbPool,
    source: SourceKind,
    package_id: &str,
) -> AppResult<Option<PackageRow>> {
    let row = sqlx::query_as::<_, PackageRow>(
        "SELECT * FROM packages WHERE source = ? AND package_id = ?",
    )
    .bind(source.to_string())
    .bind(package_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_summary(
    pool: &DbPool,
    source: SourceKind,
    package_id: &str,
) -> AppResult<Option<PackageSummary>> {
    get_row(pool, source, package_id)
        .await?
        .map(PackageSummary::try_from)
        .transpose()
}

pub async fn get_detail(
    pool: &DbPool,
    source: SourceKind,
    package_id: &str,
) -> AppResult<Option<PackageDetail>> {
    match get_row(pool, source, package_id).await? {
        Some(row) => row.into_detail(),
        None => Ok(None),
    }
}

/// Like `get_summary`, but `None` when missing or older than `ttl_secs` at `now`.
pub async fn get_fresh_summary(
    pool: &DbPool,
    source: SourceKind,
    package_id: &str,
    ttl_secs: i64,
    now: i64,
) -> AppResult<Option<PackageSummary>> {
    match get_row(pool, source, package_id).await? {
        Some(row) if now - row.fetched_at <= ttl_secs => Ok(Some(PackageSummary::try_from(row)?)),
        _ => Ok(None),
    }
}

/// Cached detail, or `None` when missing or when the detail itself (not the summary row)
/// is older than `ttl_secs` at `now`.
pub async fn get_fresh_detail(
    pool: &DbPool,
    source: SourceKind,
    package_id: &str,
    ttl_secs: i64,
    now: i64,
) -> AppResult<Option<PackageDetail>> {
    let row = sqlx::query_as::<_, PackageRow>(
        "SELECT * FROM packages WHERE source = ? AND package_id = ? \
         AND detail_fetched_at IS NOT NULL AND ? - detail_fetched_at <= ?",
    )
    .bind(source.to_string())
    .bind(package_id)
    .bind(now)
    .bind(ttl_secs)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => row.into_detail(),
        None => Ok(None),
    }
}

/// Case-insensitive substring match on name or publisher, ordered by name.
pub async fn search_cached(
    pool: &DbPool,
    text: &str,
    limit: u32,
) -> AppResult<Vec<PackageSummary>> {
    let escaped = text
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let rows = sqlx::query_as::<_, PackageRow>(
        "SELECT * FROM packages WHERE LOWER(name) LIKE ?1 ESCAPE '\\' \
         OR LOWER(publisher) LIKE ?1 ESCAPE '\\' ORDER BY name, source, package_id LIMIT ?2",
    )
    .bind(pattern)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(PackageSummary::try_from).collect()
}

/// Deletes rows with `fetched_at < cutoff_ts`; returns the number removed.
pub async fn purge_older_than(pool: &DbPool, cutoff_ts: i64) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM packages WHERE fetched_at < ?")
        .bind(cutoff_ts)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::test_pool;
    use crate::domain::package::fixtures;

    #[tokio::test]
    async fn upsert_then_get() {
        let pool = test_pool().await;
        let s = fixtures::summary(SourceKind::MsStore, "p1", "One");
        upsert_summary(&pool, &s, 100).await.unwrap();
        let got = get_summary(&pool, SourceKind::MsStore, "p1").await.unwrap();
        assert_eq!(got, Some(s));
        assert!(get_summary(&pool, SourceKind::Scoop, "p1")
            .await
            .unwrap()
            .is_none());
        assert!(get_detail(&pool, SourceKind::MsStore, "p1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn upsert_twice_updates_not_duplicates() {
        let pool = test_pool().await;
        let mut s = fixtures::summary(SourceKind::Scoop, "p", "Old");
        upsert_summary(&pool, &s, 100).await.unwrap();
        s.name = "New".into();
        s.latest_version = "9.9".into();
        upsert_summary(&pool, &s, 200).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM packages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let got = get_summary(&pool, SourceKind::Scoop, "p")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, s);
    }

    #[tokio::test]
    async fn detail_round_trip_and_summary_keeps_detail() {
        let pool = test_pool().await;
        let d = fixtures::detail(SourceKind::Chocolatey, "tool", "Tool");
        upsert_detail(&pool, &d, 100).await.unwrap();
        assert_eq!(
            get_detail(&pool, SourceKind::Chocolatey, "tool")
                .await
                .unwrap(),
            Some(d.clone())
        );
        assert_eq!(
            get_summary(&pool, SourceKind::Chocolatey, "tool")
                .await
                .unwrap(),
            Some(d.summary.clone())
        );
        let mut s2 = d.summary.clone();
        s2.latest_version = "2.0".into();
        upsert_summary(&pool, &s2, 150).await.unwrap();
        let kept = get_detail(&pool, SourceKind::Chocolatey, "tool")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kept.full_description, d.full_description);
        let sum = get_summary(&pool, SourceKind::Chocolatey, "tool")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sum.latest_version, "2.0");
    }

    #[tokio::test]
    async fn ttl_fresh_stale_missing() {
        let pool = test_pool().await;
        let s = fixtures::summary(SourceKind::Scoop, "p", "P");
        upsert_summary(&pool, &s, 1000).await.unwrap();
        let fresh = get_fresh_summary(&pool, SourceKind::Scoop, "p", 60, 1060)
            .await
            .unwrap();
        assert_eq!(fresh, Some(s));
        let stale = get_fresh_summary(&pool, SourceKind::Scoop, "p", 60, 1061)
            .await
            .unwrap();
        assert!(stale.is_none());
        let missing = get_fresh_summary(&pool, SourceKind::Scoop, "zz", 60, 1000)
            .await
            .unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn summary_refresh_does_not_refresh_detail() {
        let pool = test_pool().await;
        let d = fixtures::detail(SourceKind::Scoop, "p", "P");
        assert!(get_fresh_detail(&pool, SourceKind::Scoop, "p", 60, 0)
            .await
            .unwrap()
            .is_none());
        upsert_detail(&pool, &d, 100).await.unwrap();
        assert_eq!(
            get_fresh_detail(&pool, SourceKind::Scoop, "p", 60, 160)
                .await
                .unwrap(),
            Some(d.clone())
        );
        upsert_summary(&pool, &d.summary, 1000).await.unwrap();
        // The row is fresh again, but its detail is still from t=100.
        assert!(get_fresh_summary(&pool, SourceKind::Scoop, "p", 60, 1000)
            .await
            .unwrap()
            .is_some());
        assert!(get_fresh_detail(&pool, SourceKind::Scoop, "p", 60, 1000)
            .await
            .unwrap()
            .is_none());
        upsert_detail(&pool, &d, 1000).await.unwrap();
        assert!(get_fresh_detail(&pool, SourceKind::Scoop, "p", 60, 1000)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn search_is_case_insensitive_and_ordered() {
        let pool = test_pool().await;
        upsert_summary(
            &pool,
            &fixtures::summary(SourceKind::Scoop, "1", "Zed Editor"),
            1,
        )
        .await
        .unwrap();
        upsert_summary(
            &pool,
            &fixtures::summary(SourceKind::Scoop, "2", "alpha EDITOR"),
            1,
        )
        .await
        .unwrap();
        upsert_summary(
            &pool,
            &fixtures::summary(SourceKind::Scoop, "3", "Unrelated"),
            1,
        )
        .await
        .unwrap();
        let r = search_cached(&pool, "editor", 10).await.unwrap();
        let names: Vec<_> = r.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Zed Editor", "alpha EDITOR"]);
        assert_eq!(search_cached(&pool, "editor", 1).await.unwrap().len(), 1);
        // matches publisher too
        assert_eq!(
            search_cached(&pool, "EXAMPLE pub", 10).await.unwrap().len(),
            3
        );
        // wildcard characters are literal
        assert!(search_cached(&pool, "%", 10).await.unwrap().is_empty());
        assert!(search_cached(&pool, "nothing", 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn purge_removes_old_rows() {
        let pool = test_pool().await;
        upsert_summary(&pool, &fixtures::summary(SourceKind::Scoop, "a", "A"), 10)
            .await
            .unwrap();
        upsert_summary(&pool, &fixtures::summary(SourceKind::Scoop, "b", "B"), 20)
            .await
            .unwrap();
        assert_eq!(purge_older_than(&pool, 15).await.unwrap(), 1);
        assert!(get_summary(&pool, SourceKind::Scoop, "a")
            .await
            .unwrap()
            .is_none());
        assert!(get_summary(&pool, SourceKind::Scoop, "b")
            .await
            .unwrap()
            .is_some());
    }
}
