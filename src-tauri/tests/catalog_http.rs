//! Integration tests: the community catalog adapter and coordinator against a mock HTTP
//! server (httpmock). No real network, no winget.

use std::sync::Arc;
use std::time::Duration;

use httpmock::prelude::*;
use serde_json::json;
use store_lib::catalog::{Catalog, CatalogSource, SearchQuery};
use store_lib::domain::{PackageId, PackagingKind, SourceKind};
use store_lib::error::AppError;
use store_lib::sources::winget_repo::{user_agent, HttpPolicy, WingetRepoSource};

fn fast_policy() -> HttpPolicy {
    HttpPolicy {
        connect_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(2),
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(5),
    }
}

fn source(server: &MockServer, policy: HttpPolicy) -> WingetRepoSource {
    WingetRepoSource::with_base_url(&server.base_url(), policy).unwrap()
}

fn firefox_page() -> serde_json::Value {
    json!({
        "Packages": [
            {
                "Id": "Mozilla.Firefox",
                "Versions": ["110.0.1", "110.0"],
                "Latest": {
                    "Name": "Mozilla Firefox",
                    "Publisher": "Mozilla",
                    "Tags": ["browser"],
                    "Description": "Firefox Browser, a free and open-source web browser.\nMore.",
                    "Homepage": "https://www.mozilla.org/",
                    "License": "MPL-2.0"
                },
                "Featured": false,
                "IconUrl": null,
                "SearchScore": 15.4
            },
            {
                "Id": "filips.FirefoxPWA",
                "Versions": ["2.4.1"],
                "Latest": { "Name": "PWAsForFirefox", "Publisher": "filips" },
                "IconUrl": "https://example.org/pwa.png"
            }
        ],
        "Total": 2
    })
}

#[tokio::test]
async fn search_returns_typed_summaries_without_winget() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/packages")
                .query_param("query", "firefox")
                .query_param("take", "30")
                .header("user-agent", user_agent());
            then.status(200)
                .header("content-type", "application/json")
                .json_body(firefox_page());
        })
        .await;

    let src = source(&server, fast_policy());
    let rows = src.search(&SearchQuery::new("  Firefox ")).await.unwrap();

    mock.assert_async().await;
    assert_eq!(rows.len(), 2);
    let ff = &rows[0];
    assert_eq!(
        ff.id,
        PackageId::new(SourceKind::WingetCommunity, "Mozilla.Firefox")
    );
    assert_eq!(ff.name, "Mozilla Firefox");
    assert_eq!(ff.publisher, "Mozilla");
    assert_eq!(ff.latest_version, "110.0.1");
    assert_eq!(
        ff.description,
        "Firefox Browser, a free and open-source web browser."
    );
    assert_eq!(ff.source, SourceKind::WingetCommunity);
    assert_eq!(ff.packaging, PackagingKind::Unknown);
    assert!(!ff.is_paid);
    assert_eq!(
        rows[1].icon_url.as_deref(),
        Some("https://example.org/pwa.png")
    );
}

#[tokio::test]
async fn detail_is_fetched_by_publisher_and_name() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages/Mozilla/Firefox");
            then.status(200)
                .json_body(json!({ "Package": firefox_page()["Packages"][0] }));
        })
        .await;

    let d = source(&server, fast_policy())
        .get_detail("Mozilla.Firefox")
        .await
        .unwrap();
    mock.assert_async().await;
    assert_eq!(d.summary.name, "Mozilla Firefox");
    assert_eq!(d.website.as_deref(), Some("https://www.mozilla.org/"));
    assert_eq!(d.versions.len(), 2);
}

#[tokio::test]
async fn missing_package_maps_to_package_not_found_without_retry() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages/Nope/Nothing");
            then.status(404)
                .json_body(json!({"statusCode": 404, "message": "package not found"}));
        })
        .await;
    let err = source(&server, fast_policy())
        .get_detail("Nope.Nothing")
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::PackageNotFound(ref id) if id == "Nope.Nothing"));
    assert!(!err.retryable());
    assert_eq!(mock.calls_async().await, 1);

    // An id without a publisher part never reaches the server.
    let err = source(&server, fast_policy())
        .get_detail("nodot")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "package_not_found");
}

#[tokio::test]
async fn malformed_json_maps_to_source_unavailable_without_retry() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(200).body("<html>not json");
        })
        .await;
    let err = source(&server, fast_policy())
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert!(err.to_string().contains("unreadable catalog response"));
    assert_eq!(mock.calls_async().await, 1);
}

#[tokio::test]
async fn wrong_json_shape_maps_to_source_unavailable() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(200).json_body(json!({"Packages": "nope"}));
        })
        .await;
    let err = source(&server, fast_policy())
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
}

#[tokio::test]
async fn rate_limit_retries_with_backoff_then_fails_cleanly() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(429).header("retry-after", "0");
        })
        .await;
    let err = source(&server, fast_policy())
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert!(err.to_string().contains("rate limited"));
    assert!(err.retryable());
    assert_eq!(mock.calls_async().await, 3);
}

#[tokio::test]
async fn server_errors_are_retried() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(503);
        })
        .await;
    let err = source(&server, fast_policy())
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert_eq!(mock.calls_async().await, 3);
}

#[tokio::test]
async fn slow_server_maps_to_retryable_http_error() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(200)
                .delay(Duration::from_millis(500))
                .json_body(firefox_page());
        })
        .await;
    let policy = HttpPolicy {
        request_timeout: Duration::from_millis(100),
        max_attempts: 2,
        ..fast_policy()
    };
    let err = source(&server, policy)
        .search(&SearchQuery::new("firefox"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "http");
    assert!(err.retryable());
    assert_eq!(mock.calls_async().await, 2);
}

async fn memory_pool() -> store_lib::db::DbPool {
    let pool = store_lib::db::init_memory_pool().await.unwrap();
    store_lib::db::run_migrations(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn coordinator_caches_and_falls_back_to_cached_rows() {
    let server = MockServer::start_async().await;
    let ok = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(200).json_body(firefox_page());
        })
        .await;
    let pool = memory_pool().await;
    let src: Arc<dyn CatalogSource> = Arc::new(source(&server, fast_policy()));

    // Fresh results are served from the query cache within the TTL.
    let catalog = Catalog::new(vec![Arc::clone(&src)], Some(pool.clone()), 3600);
    let first = catalog.search(&SearchQuery::new("firefox")).await.unwrap();
    let second = catalog.search(&SearchQuery::new("FIREFOX")).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(ok.calls_async().await, 1);

    // Rows were persisted; with the server down a new coordinator serves them from SQLite.
    ok.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages");
            then.status(503);
        })
        .await;
    let offline = Catalog::new(vec![src], Some(pool), 0);
    let cached = offline.search(&SearchQuery::new("firefox")).await.unwrap();
    let names: Vec<_> = cached.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Mozilla Firefox", "PWAsForFirefox"]);

    // Without a cache the error surfaces as-is.
    let err = offline
        .search(&SearchQuery::new("no such thing"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
}

#[tokio::test]
async fn coordinator_detail_uses_ttl_cache() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(GET).path("/v2/packages/Mozilla/Firefox");
            then.status(200)
                .json_body(json!({ "Package": firefox_page()["Packages"][0] }));
        })
        .await;
    let pool = memory_pool().await;
    let src: Arc<dyn CatalogSource> = Arc::new(source(&server, fast_policy()));
    let id = PackageId::new(SourceKind::WingetCommunity, "Mozilla.Firefox");

    let catalog = Catalog::new(vec![Arc::clone(&src)], Some(pool.clone()), 3600);
    let a = catalog.get_detail(&id).await.unwrap();
    let b = catalog.get_detail(&id).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(mock.calls_async().await, 1);

    // TTL 0: always refetch.
    let uncached = Catalog::new(vec![src], Some(pool), 0);
    uncached.get_detail(&id).await.unwrap();
    assert_eq!(mock.calls_async().await, 2);

    // A source that is not enabled is reported, not guessed.
    let err = uncached
        .get_detail(&PackageId::new(SourceKind::Scoop, "x"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
}
