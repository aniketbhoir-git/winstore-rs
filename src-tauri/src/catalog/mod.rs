//! Catalog layer: the `CatalogSource` contract (SPEC §5.1) and the coordinator that fans a
//! search out over the enabled sources, merges, caches and pages the results (SPEC §5.3, §6.1).

pub mod search;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use tokio::task::JoinSet;

use crate::db::{repo::packages, DbPool};
use crate::domain::{
    now_unix, PackageDetail, PackageId, PackageSummary, SourceCapabilities, SourceKind,
};
use crate::error::{AppError, AppResult};

pub use search::{SearchQuery, SortOrder};

/// Boxed `Send` future, so `CatalogSource` stays object-safe without extra crates.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A read-only catalog of packages (SPEC §5.1). Timeouts and rate limits live in each adapter.
///
/// `package_id` arguments are source-local ids (the part after `{source}/` in a `PackageId`).
pub trait CatalogSource: Send + Sync {
    fn kind(&self) -> SourceKind;

    fn display_name(&self) -> &str {
        self.kind().display_name()
    }

    fn capabilities(&self) -> SourceCapabilities;

    /// Returns at most `query.fetch_limit()` summaries, best match first.
    fn search<'a>(
        &'a self,
        query: &'a SearchQuery,
    ) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>>;

    fn get_detail<'a>(&'a self, package_id: &'a str) -> BoxFuture<'a, AppResult<PackageDetail>>;

    fn get_featured(&self, _limit: u32) -> BoxFuture<'_, AppResult<Vec<PackageSummary>>> {
        let name = self.display_name().to_string();
        Box::pin(async move { Err(AppError::Unsupported(format!("featured list for {name}"))) })
    }

    fn list_categories(&self) -> BoxFuture<'_, AppResult<Vec<String>>> {
        let name = self.display_name().to_string();
        Box::pin(async move { Err(AppError::Unsupported(format!("categories for {name}"))) })
    }
}

/// Cache key for one source's answer to one query: (source, normalized text, fetch limit).
type QueryKey = (SourceKind, String, u32);

/// Coordinator over the enabled sources, in priority order.
pub struct Catalog {
    sources: Vec<Arc<dyn CatalogSource>>,
    pool: Option<DbPool>,
    ttl_secs: i64,
    /// Per-source search answers with the time they were fetched.
    query_cache: Mutex<HashMap<QueryKey, (i64, Vec<PackageSummary>)>>,
}

impl Catalog {
    /// `sources` are the enabled sources in priority order. `ttl_secs == 0` disables serving
    /// fresh results from the cache; cached rows are still used when a source is unreachable.
    pub fn new(sources: Vec<Arc<dyn CatalogSource>>, pool: Option<DbPool>, ttl_secs: u64) -> Self {
        Catalog {
            sources,
            pool,
            ttl_secs: i64::try_from(ttl_secs).unwrap_or(i64::MAX),
            query_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn sources(&self) -> &[Arc<dyn CatalogSource>] {
        &self.sources
    }

    fn source(&self, kind: SourceKind) -> AppResult<&Arc<dyn CatalogSource>> {
        self.sources
            .iter()
            .find(|s| s.kind() == kind)
            .ok_or_else(|| {
                AppError::SourceUnavailable(format!("{} is not enabled", kind.display_name()))
            })
    }

    /// Same rule as `packages::get_fresh_summary`: fresh while `now - fetched_at <= ttl`.
    fn is_fresh(&self, fetched_at: i64, now: i64) -> bool {
        self.ttl_secs > 0 && now.saturating_sub(fetched_at) <= self.ttl_secs
    }

    /// Searches every enabled source concurrently, then merges, dedupes, sorts and pages.
    ///
    /// A failing source is replaced by its cached rows when there are any; the call fails
    /// only when every source failed and nothing was cached.
    pub async fn search(&self, query: &SearchQuery) -> AppResult<Vec<PackageSummary>> {
        let text = query.normalized_text();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let now = now_unix();
        let limit = query.fetch_limit();
        let targets: Vec<_> = self
            .sources
            .iter()
            .filter(|s| query.source.is_none_or(|k| k == s.kind()) && s.capabilities().search)
            .cloned()
            .collect();

        let mut results: Vec<Option<AppResult<Vec<PackageSummary>>>> =
            targets.iter().map(|_| None).collect();
        let mut tasks = JoinSet::new();
        for (i, src) in targets.iter().enumerate() {
            let key = (src.kind(), text.clone(), limit);
            if let Some(hit) = self.cached_query(&key, now) {
                results[i] = Some(Ok(hit));
                continue;
            }
            let src = Arc::clone(src);
            let q = query.clone();
            tasks.spawn(async move { (i, key, src.search(&q).await) });
        }
        while let Some(joined) = tasks.join_next().await {
            let (i, key, result) = joined
                .map_err(|e| AppError::SourceUnavailable(format!("search task failed: {e}")))?;
            if let Ok(rows) = &result {
                self.persist_summaries(rows, now).await;
                self.store_query(key, now, rows.clone());
            }
            results[i] = Some(result);
        }

        let mut per_source = Vec::with_capacity(targets.len());
        let mut first_error = None;
        for (src, result) in targets.iter().zip(results) {
            let Some(result) = result else { continue };
            match result {
                Ok(rows) => per_source.push(rows),
                Err(e) => {
                    tracing::warn!(source = %src.kind(), "catalog search failed: {e}");
                    match self.cached_rows(src.kind(), &text, limit).await {
                        Some(rows) if !rows.is_empty() => per_source.push(rows),
                        _ => {
                            first_error.get_or_insert(e);
                        }
                    }
                }
            }
        }
        if per_source.is_empty() {
            if let Some(e) = first_error {
                return Err(e);
            }
        }
        let mut merged = search::merge(per_source);
        search::sort(&mut merged, query.sort);
        Ok(search::paginate(merged, query))
    }

    /// Full detail for a `{source}/{package_id}` id, served from the cache while fresh.
    pub async fn get_detail(&self, id: &PackageId) -> AppResult<PackageDetail> {
        let (kind, local) = PackageId::parse(id.as_str())?;
        let src = self.source(kind)?;
        let now = now_unix();
        if let Some(detail) = self.cached_detail(kind, &local, Some(now)).await {
            return Ok(detail);
        }
        match src.get_detail(&local).await {
            Ok(detail) => {
                if let Some(pool) = &self.pool {
                    if let Err(e) = packages::upsert_detail(pool, &detail, now).await {
                        tracing::warn!(%id, "could not cache package detail: {e}");
                    }
                }
                Ok(detail)
            }
            Err(e) if e.retryable() => {
                // Offline or rate limited: a stale cached detail beats an error.
                match self.cached_detail(kind, &local, None).await {
                    Some(detail) => {
                        tracing::warn!(%id, "serving stale cached detail: {e}");
                        Ok(detail)
                    }
                    None => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    fn cached_query(&self, key: &QueryKey, now: i64) -> Option<Vec<PackageSummary>> {
        let cache = self.query_cache.lock().ok()?;
        match cache.get(key) {
            Some((at, rows)) if self.is_fresh(*at, now) => Some(rows.clone()),
            _ => None,
        }
    }

    fn store_query(&self, key: QueryKey, now: i64, rows: Vec<PackageSummary>) {
        if self.ttl_secs == 0 {
            return;
        }
        if let Ok(mut cache) = self.query_cache.lock() {
            cache.retain(|_, (at, _)| self.is_fresh(*at, now));
            cache.insert(key, (now, rows));
        }
    }

    async fn persist_summaries(&self, rows: &[PackageSummary], now: i64) {
        let Some(pool) = &self.pool else { return };
        for row in rows {
            if let Err(e) = packages::upsert_summary(pool, row, now).await {
                tracing::warn!(id = %row.id, "could not cache package summary: {e}");
            }
        }
    }

    /// Cached rows of one source matching `text`, regardless of age (offline fallback).
    async fn cached_rows(
        &self,
        kind: SourceKind,
        text: &str,
        limit: u32,
    ) -> Option<Vec<PackageSummary>> {
        let pool = self.pool.as_ref()?;
        match packages::search_cached(pool, text, search::MAX_PAGE_SIZE * 4).await {
            Ok(rows) => Some(
                rows.into_iter()
                    .filter(|r| r.source == kind)
                    .take(limit as usize)
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!("cache lookup failed: {e}");
                None
            }
        }
    }

    /// Cached detail; with `fresh_at`, only when it is within the TTL at that time.
    async fn cached_detail(
        &self,
        kind: SourceKind,
        local: &str,
        fresh_at: Option<i64>,
    ) -> Option<PackageDetail> {
        let pool = self.pool.as_ref()?;
        if let Some(now) = fresh_at {
            if self.ttl_secs == 0 {
                return None;
            }
            packages::get_fresh_summary(pool, kind, local, self.ttl_secs, now)
                .await
                .ok()??;
        }
        packages::get_detail(pool, kind, local).await.ok()?
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::domain::package::fixtures;

    /// In-memory source: fixed results or a fixed error, counting calls.
    struct FakeSource {
        kind: SourceKind,
        rows: Option<Vec<PackageSummary>>,
        calls: AtomicUsize,
    }

    impl FakeSource {
        fn ok(kind: SourceKind, rows: Vec<PackageSummary>) -> Arc<Self> {
            Arc::new(FakeSource {
                kind,
                rows: Some(rows),
                calls: AtomicUsize::new(0),
            })
        }

        fn failing(kind: SourceKind) -> Arc<Self> {
            Arc::new(FakeSource {
                kind,
                rows: None,
                calls: AtomicUsize::new(0),
            })
        }
    }

    impl CatalogSource for FakeSource {
        fn kind(&self) -> SourceKind {
            self.kind
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities {
                search: true,
                detail: true,
                ..SourceCapabilities::default()
            }
        }

        fn search<'a>(
            &'a self,
            _query: &'a SearchQuery,
        ) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self
                .rows
                .clone()
                .ok_or_else(|| AppError::SourceUnavailable("down".into()));
            Box::pin(async move { result })
        }

        fn get_detail<'a>(
            &'a self,
            package_id: &'a str,
        ) -> BoxFuture<'a, AppResult<PackageDetail>> {
            let kind = self.kind;
            Box::pin(async move { Ok(fixtures::detail(kind, package_id, "Detail")) })
        }
    }

    fn row(kind: SourceKind, id: &str, name: &str) -> PackageSummary {
        fixtures::summary(kind, id, name)
    }

    #[tokio::test]
    async fn merges_sources_in_priority_order_and_dedupes() {
        let a = FakeSource::ok(
            SourceKind::WingetCommunity,
            vec![row(SourceKind::WingetCommunity, "A.App", "App")],
        );
        let b = FakeSource::ok(
            SourceKind::Scoop,
            vec![
                row(SourceKind::Scoop, "app", "app"),
                row(SourceKind::Scoop, "other", "Other"),
            ],
        );
        let catalog = Catalog::new(vec![a, b], None, 0);
        let rows = catalog.search(&SearchQuery::new("app")).await.unwrap();
        let ids: Vec<_> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["winget_community/A.App", "scoop/other"]);
    }

    #[tokio::test]
    async fn source_filter_and_empty_query() {
        let a = FakeSource::ok(
            SourceKind::WingetCommunity,
            vec![row(SourceKind::WingetCommunity, "A", "A")],
        );
        let b = FakeSource::ok(SourceKind::Scoop, vec![row(SourceKind::Scoop, "b", "B")]);
        let catalog = Catalog::new(vec![a.clone(), b.clone()], None, 0);
        let mut q = SearchQuery::new("x");
        q.source = Some(SourceKind::Scoop);
        let rows = catalog.search(&q).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, SourceKind::Scoop);
        assert_eq!(a.calls.load(Ordering::SeqCst), 0);

        assert!(catalog
            .search(&SearchQuery::new("   "))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(b.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn one_failing_source_does_not_fail_the_search() {
        let good = FakeSource::ok(SourceKind::Scoop, vec![row(SourceKind::Scoop, "b", "B")]);
        let bad = FakeSource::failing(SourceKind::WingetCommunity);
        let catalog = Catalog::new(vec![bad.clone(), good], None, 0);
        let rows = catalog.search(&SearchQuery::new("b")).await.unwrap();
        assert_eq!(rows.len(), 1);

        let only_bad = Catalog::new(vec![bad], None, 0);
        let err = only_bad.search(&SearchQuery::new("b")).await.unwrap_err();
        assert_eq!(err.code(), "source_unavailable");
    }

    #[tokio::test]
    async fn query_cache_respects_ttl_and_persists_rows() {
        let pool = crate::db::repo::test_pool().await;
        let src = FakeSource::ok(SourceKind::Scoop, vec![row(SourceKind::Scoop, "b", "B")]);
        let cached = Catalog::new(vec![src.clone()], Some(pool.clone()), 60);
        cached.search(&SearchQuery::new("b")).await.unwrap();
        cached.search(&SearchQuery::new(" B ")).await.unwrap();
        assert_eq!(src.calls.load(Ordering::SeqCst), 1);
        assert!(packages::get_summary(&pool, SourceKind::Scoop, "b")
            .await
            .unwrap()
            .is_some());

        let uncached = Catalog::new(vec![src.clone()], Some(pool), 0);
        uncached.search(&SearchQuery::new("b")).await.unwrap();
        uncached.search(&SearchQuery::new("b")).await.unwrap();
        assert_eq!(src.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn detail_routes_to_the_owning_source() {
        let catalog = Catalog::new(vec![FakeSource::ok(SourceKind::Scoop, vec![])], None, 0);
        let id = catalog
            .get_detail(&PackageId::new(SourceKind::Scoop, "x"))
            .await
            .map(|d| d.summary.id);
        assert_eq!(id.unwrap().as_str(), "scoop/x");
    }

    #[tokio::test]
    async fn default_featured_and_categories_are_unsupported() {
        let src = FakeSource::ok(SourceKind::Scoop, vec![]);
        assert_eq!(src.get_featured(5).await.unwrap_err().code(), "unsupported");
        assert_eq!(
            src.list_categories().await.unwrap_err().code(),
            "unsupported"
        );
        assert_eq!(src.display_name(), "Scoop");
    }
}
