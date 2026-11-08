//! winget community catalog over the public `winget.run` REST API (SPEC §0, WP2).
//!
//! Endpoints (read-only, HTTPS, no auth):
//! - `GET /v2/packages?query={text}&take={n}` -> `{"Packages": [...], "Total": n}`
//! - `GET /v2/packages/{publisher}/{name}` -> `{"Package": {...}}`, where `{publisher}.{name}`
//!   is the winget package identifier split at its first dot.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{header, Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::catalog::{BoxFuture, CatalogSource, SearchQuery};
use crate::domain::{
    InstallerMetadata, PackageDetail, PackageId, PackageSummary, PackageVersion, PackagingKind,
    SourceCapabilities, SourceKind,
};
use crate::error::{AppError, AppResult};

/// Public endpoint of the community REST catalog.
pub const DEFAULT_BASE_URL: &str = "https://api.winget.run";

/// Longest one-line description kept in a summary, in characters.
const SUMMARY_DESCRIPTION_CHARS: usize = 160;

/// Sent with every request so catalog operators can identify the client.
pub fn user_agent() -> String {
    format!(
        "Store/{} (+https://github.com/aniketbhoir-git/winstore-rs)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Timeouts and retry/backoff for catalog requests.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpPolicy {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// Total attempts, including the first one (at least 1).
    pub max_attempts: u32,
    /// First backoff delay; doubles on every retry.
    pub base_delay: Duration,
    /// Upper bound for one backoff delay, including a server `Retry-After`.
    pub max_delay: Duration,
    /// Largest response body accepted; a bigger one maps to `SourceUnavailable`.
    pub max_body_bytes: usize,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        HttpPolicy {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(20),
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            max_body_bytes: 8 * 1024 * 1024,
        }
    }
}

impl HttpPolicy {
    /// Delay before retry number `retry` (1-based), honouring `Retry-After` when given.
    fn backoff(&self, retry: u32, retry_after: Option<Duration>) -> Duration {
        let exp = self
            .base_delay
            .saturating_mul(2u32.saturating_pow(retry.saturating_sub(1)));
        retry_after.unwrap_or(exp).min(self.max_delay)
    }
}

/// Outcome of one HTTP attempt.
enum Attempt<T> {
    Done(AppResult<T>),
    Retry(AppError, Option<Duration>),
}

pub struct WingetRepoSource {
    client: Client,
    base_url: Url,
    policy: HttpPolicy,
}

impl WingetRepoSource {
    /// Source for the public endpoint with the default policy.
    pub fn new() -> AppResult<Self> {
        Self::with_base_url(DEFAULT_BASE_URL, HttpPolicy::default())
    }

    /// Source for another base URL. Only HTTPS is accepted, except plain HTTP to a
    /// loopback host (local mirrors and tests).
    pub fn with_base_url(base_url: &str, policy: HttpPolicy) -> AppResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|e| AppError::InvalidConfig(format!("invalid catalog URL {base_url}: {e}")))?;
        let loopback = matches!(
            base_url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
        let allowed = base_url.scheme() == "https" || (base_url.scheme() == "http" && loopback);
        if !allowed || base_url.cannot_be_a_base() {
            return Err(AppError::InvalidConfig(format!(
                "catalog URL must use HTTPS: {base_url}"
            )));
        }
        let client = Client::builder()
            .user_agent(user_agent())
            // Redirects must stay on HTTPS too; plain HTTP only for a loopback base URL.
            .https_only(!loopback)
            .connect_timeout(policy.connect_timeout)
            .timeout(policy.request_timeout)
            .build()
            .map_err(|e| AppError::Http(format!("could not build HTTP client: {e}")))?;
        Ok(WingetRepoSource {
            client,
            base_url,
            policy,
        })
    }

    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base_url.clone();
        // `with_base_url` rejects cannot-be-a-base URLs, so this always succeeds.
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().extend(segments);
        }
        url
    }

    fn unavailable(&self, what: &str) -> AppError {
        AppError::SourceUnavailable(format!("{}: {what}", self.display_name()))
    }

    /// GET with timeout, retry on transport errors, 429 and 5xx, and exponential backoff.
    /// A 404 maps to `PackageNotFound(id)` when `not_found` is `Some(id)` (detail lookups),
    /// otherwise to `SourceUnavailable`. Other 4xx statuses are `SourceUnavailable`, not retried.
    async fn get_json<T: DeserializeOwned>(
        &self,
        url: Url,
        not_found: Option<&str>,
    ) -> AppResult<T> {
        let attempts = self.policy.max_attempts.max(1);
        let mut attempt = 1;
        loop {
            match self.try_once(url.clone(), not_found).await {
                Attempt::Done(result) => return result,
                Attempt::Retry(err, retry_after) => {
                    if attempt >= attempts {
                        return Err(err);
                    }
                    let delay = self.policy.backoff(attempt, retry_after);
                    tracing::debug!(%url, attempt, ?delay, "retrying catalog request: {err}");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn try_once<T: DeserializeOwned>(&self, url: Url, not_found: Option<&str>) -> Attempt<T> {
        let response = match self
            .client
            .get(url)
            .header(header::ACCEPT, "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) if e.is_timeout() => {
                return Attempt::Retry(AppError::Http("catalog request timed out".into()), None)
            }
            Err(e) => {
                return Attempt::Retry(AppError::Http(format!("catalog request failed: {e}")), None)
            }
        };
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            // Only the delay-seconds form is honoured; an HTTP-date `Retry-After` falls back
            // to the normal exponential backoff.
            let retry_after = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            return Attempt::Retry(
                self.unavailable("rate limited, try again later"),
                retry_after,
            );
        }
        if status.is_server_error() {
            return Attempt::Retry(self.unavailable(&format!("server error {status}")), None);
        }
        if status == StatusCode::NOT_FOUND {
            return Attempt::Done(Err(match not_found {
                Some(id) => AppError::PackageNotFound(id.to_string()),
                None => self.unavailable("endpoint not found (404)"),
            }));
        }
        if !status.is_success() {
            return Attempt::Done(Err(self.unavailable(&format!("unexpected status {status}"))));
        }
        let cap = self.policy.max_body_bytes;
        let too_large = || {
            Attempt::Done(Err(
                self.unavailable(&format!("response larger than {cap} bytes"))
            ))
        };
        if response.content_length().is_some_and(|n| n > cap as u64) {
            return too_large();
        }
        let mut response = response;
        let mut body = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > cap {
                        return too_large();
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) if e.is_timeout() => {
                    return Attempt::Retry(AppError::Http("catalog request timed out".into()), None)
                }
                Err(e) => {
                    return Attempt::Retry(
                        AppError::Http(format!("catalog response failed: {e}")),
                        None,
                    )
                }
            }
        }
        Attempt::Done(
            serde_json::from_slice(&body)
                .map_err(|e| self.unavailable(&format!("unreadable catalog response ({e})"))),
        )
    }

    async fn search_impl(&self, query: &SearchQuery) -> AppResult<Vec<PackageSummary>> {
        let text = query.normalized_text();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let mut url = self.url(&["v2", "packages"]);
        url.query_pairs_mut()
            .append_pair("query", &text)
            .append_pair("take", &query.fetch_limit().to_string());
        let page: SearchResponse = self.get_json(url, None).await?;
        Ok(page
            .packages
            .into_iter()
            .filter_map(ApiPackage::into_summary)
            .take(query.fetch_limit() as usize)
            .collect())
    }

    async fn detail_impl(&self, package_id: &str) -> AppResult<PackageDetail> {
        let not_found = || AppError::PackageNotFound(package_id.to_string());
        let (publisher, name) = package_id
            .split_once('.')
            .filter(|(p, n)| !p.is_empty() && !n.is_empty())
            .ok_or_else(not_found)?;
        let url = self.url(&["v2", "packages", publisher, name]);
        let res: DetailResponse = self.get_json(url, Some(package_id)).await?;
        res.package
            .into_detail()
            .ok_or_else(|| self.unavailable(&format!("incomplete catalog entry for {package_id}")))
    }
}

impl CatalogSource for WingetRepoSource {
    fn kind(&self) -> SourceKind {
        SourceKind::WingetCommunity
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            search: true,
            detail: true,
            install: true,
            update: true,
            pin: true,
            ..SourceCapabilities::default()
        }
    }

    fn search<'a>(
        &'a self,
        query: &'a SearchQuery,
    ) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>> {
        Box::pin(self.search_impl(query))
    }

    fn get_detail<'a>(&'a self, package_id: &'a str) -> BoxFuture<'a, AppResult<PackageDetail>> {
        Box::pin(self.detail_impl(package_id))
    }
}

// ---- wire format -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(rename = "Packages")]
    packages: Vec<ApiPackage>,
}

#[derive(Debug, Deserialize)]
struct DetailResponse {
    #[serde(rename = "Package")]
    package: ApiPackage,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ApiPackage {
    id: Option<String>,
    /// Newest first.
    versions: Vec<String>,
    latest: ApiLatest,
    icon_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ApiLatest {
    name: Option<String>,
    publisher: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// First line of `text`, cut to `SUMMARY_DESCRIPTION_CHARS` characters.
fn one_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= SUMMARY_DESCRIPTION_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(SUMMARY_DESCRIPTION_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

impl ApiPackage {
    /// `None` when the entry lacks an id or a name; such entries are skipped.
    fn into_summary(self) -> Option<PackageSummary> {
        self.split().map(|(summary, _)| summary)
    }

    fn into_detail(self) -> Option<PackageDetail> {
        let (summary, rest) = self.split()?;
        let versions = rest
            .versions
            .iter()
            .map(|v| {
                (
                    v.clone(),
                    PackageVersion {
                        version: v.clone(),
                        release_date: None,
                        installer_url: None,
                        sha256: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        Some(PackageDetail {
            summary,
            full_description: rest.description.unwrap_or_default(),
            screenshots: Vec::new(),
            website: rest.homepage,
            license: rest.license,
            release_notes: None,
            installer: InstallerMetadata {
                architecture: None,
                install_type: PackagingKind::Unknown,
                size_bytes: None,
                sha256: None,
            },
            versions,
        })
    }

    fn split(self) -> Option<(PackageSummary, DetailExtras)> {
        let id = non_empty(self.id)?;
        let name = non_empty(self.latest.name)?;
        let description = non_empty(self.latest.description);
        let versions: Vec<String> = self
            .versions
            .into_iter()
            .filter_map(|v| non_empty(Some(v)))
            .collect();
        let summary = PackageSummary {
            id: PackageId::new(SourceKind::WingetCommunity, &id),
            name,
            publisher: non_empty(self.latest.publisher).unwrap_or_default(),
            latest_version: versions.first().cloned().unwrap_or_default(),
            description: description.as_deref().map(one_line).unwrap_or_default(),
            icon_url: non_empty(self.icon_url),
            rating: None,
            rating_count: None,
            source: SourceKind::WingetCommunity,
            is_paid: false,
            packaging: PackagingKind::Unknown,
        };
        let extras = DetailExtras {
            versions,
            description,
            homepage: non_empty(self.latest.homepage),
            license: non_empty(self.latest.license),
        };
        Some((summary, extras))
    }
}

struct DetailExtras {
    versions: Vec<String>,
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_JSON: &str = r#"{"Packages":[
        {"Id":"Mozilla.Firefox","Versions":["110.0.1","110.0"],
         "Latest":{"Name":"Mozilla Firefox","Publisher":"Mozilla","Tags":["browser"],
                   "Description":"Firefox Browser.\nSecond line.","Homepage":"https://www.mozilla.org/",
                   "License":"MPL-2.0"},
         "Featured":false,"IconUrl":null,"SearchScore":15.4},
        {"Id":"","Latest":{"Name":"No id"}},
        {"Id":"No.Name","Latest":{}}
    ],"Total":3}"#;

    #[test]
    fn parses_search_page_and_skips_incomplete_entries() {
        let page: SearchResponse = serde_json::from_str(SEARCH_JSON).unwrap();
        let rows: Vec<_> = page
            .packages
            .into_iter()
            .filter_map(ApiPackage::into_summary)
            .collect();
        assert_eq!(rows.len(), 1);
        let s = &rows[0];
        assert_eq!(s.id.as_str(), "winget_community/Mozilla.Firefox");
        assert_eq!(s.name, "Mozilla Firefox");
        assert_eq!(s.publisher, "Mozilla");
        assert_eq!(s.latest_version, "110.0.1");
        assert_eq!(s.description, "Firefox Browser.");
        assert_eq!(s.icon_url, None);
        assert_eq!(s.source, SourceKind::WingetCommunity);
        assert!(!s.is_paid);
        assert_eq!(s.packaging, PackagingKind::Unknown);
    }

    #[test]
    fn parses_detail() {
        let json = r#"{"Package":{"Id":"Mozilla.Firefox","Versions":["110.0.1","109.0"],
            "Latest":{"Name":"Mozilla Firefox","Publisher":"Mozilla",
            "Description":"Line one.\nLine two.","Homepage":"https://www.mozilla.org/",
            "License":"MPL-2.0"}}}"#;
        let res: DetailResponse = serde_json::from_str(json).unwrap();
        let d = res.package.into_detail().unwrap();
        assert_eq!(d.summary.latest_version, "110.0.1");
        assert_eq!(d.full_description, "Line one.\nLine two.");
        assert_eq!(d.website.as_deref(), Some("https://www.mozilla.org/"));
        assert_eq!(d.license.as_deref(), Some("MPL-2.0"));
        assert_eq!(d.versions.keys().collect::<Vec<_>>(), ["109.0", "110.0.1"]);
        assert!(d.screenshots.is_empty());
    }

    #[test]
    fn one_line_truncates_long_text() {
        assert_eq!(one_line("\n  first \nsecond"), "first");
        let long = "x".repeat(500);
        let cut = one_line(&long);
        assert_eq!(cut.chars().count(), SUMMARY_DESCRIPTION_CHARS);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn backoff_doubles_and_is_capped() {
        let p = HttpPolicy {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(350),
            ..HttpPolicy::default()
        };
        assert_eq!(p.backoff(1, None), Duration::from_millis(100));
        assert_eq!(p.backoff(2, None), Duration::from_millis(200));
        assert_eq!(p.backoff(3, None), Duration::from_millis(350));
        assert_eq!(
            p.backoff(1, Some(Duration::from_secs(60))),
            Duration::from_millis(350)
        );
        assert_eq!(p.backoff(40, None), Duration::from_millis(350));
    }

    #[test]
    fn only_https_or_loopback_http_is_accepted() {
        let p = HttpPolicy::default;
        assert!(WingetRepoSource::with_base_url(DEFAULT_BASE_URL, p()).is_ok());
        assert!(WingetRepoSource::with_base_url("http://127.0.0.1:8080", p()).is_ok());
        assert!(WingetRepoSource::with_base_url("http://localhost:1", p()).is_ok());
        for bad in [
            "http://example.org",
            "ftp://example.org",
            "not a url",
            "data:x",
        ] {
            let e = WingetRepoSource::with_base_url(bad, p()).err().unwrap();
            assert_eq!(e.code(), "invalid_config", "{bad}");
        }
    }

    #[test]
    fn urls_are_built_from_escaped_segments() {
        let src =
            WingetRepoSource::with_base_url("https://example.org/api/", HttpPolicy::default())
                .unwrap();
        assert_eq!(
            src.url(&["v2", "packages", "Pub", "A b/c"]).as_str(),
            "https://example.org/api/v2/packages/Pub/A%20b%2Fc"
        );
    }

    #[test]
    fn user_agent_names_the_app() {
        assert!(user_agent().starts_with("Store/"));
        assert!(user_agent().contains(env!("CARGO_PKG_VERSION")));
    }
}
