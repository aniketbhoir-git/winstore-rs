//! winget community catalog through the winget CLI (ADR 0001 option A, design
//! `docs/designs/catalog-winget-cli.md`). Read-only: `winget search` and `winget show` with
//! `--source winget`, never an agreement flag. Not wired into `enabled_sources()` yet (plan step 4).
//!
//! The source keeps `SourceKind::WingetCommunity`, so package ids stay `winget_community/<id>`.
//! It connects to winget lazily, at the first call, so an app without winget still starts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::OnceCell;

use crate::catalog::{BoxFuture, CatalogSource, SearchQuery};
use crate::domain::{
    InstallerMetadata, PackageDetail, PackageId, PackageSummary, PackageVersion, PackagingKind,
    SourceCapabilities, SourceKind,
};
use crate::error::{AppError, AppResult};
use crate::executor::winget_cli::{WingetCli, WingetOptions};
use crate::executor::winget_parse::{SearchRow, ShowBlock};

/// Longest one-line description kept in a summary, in characters.
const SUMMARY_DESCRIPTION_CHARS: usize = 160;

enum Location {
    /// Discover winget (`STORE_WINGET_PATH`, PATH, WindowsApps).
    Discover,
    Path(PathBuf),
}

pub struct WingetCliSource {
    cli: OnceCell<WingetCli>,
    location: Location,
    options: WingetOptions,
    limits: Option<(Duration, Duration)>,
}

impl WingetCliSource {
    /// Discovers winget at the first call.
    pub fn new() -> Self {
        WingetCliSource {
            cli: OnceCell::new(),
            location: Location::Discover,
            options: WingetOptions::default(),
            limits: None,
        }
    }

    /// Uses a known winget binary (tests point this at the fake).
    pub fn with_path(path: impl Into<PathBuf>, options: WingetOptions) -> Self {
        WingetCliSource {
            cli: OnceCell::new(),
            location: Location::Path(path.into()),
            options,
            limits: None,
        }
    }

    /// Replaces the search and show time limits (test seam; set before the first call).
    pub fn with_limits(mut self, search: Duration, show: Duration) -> Self {
        self.limits = Some((search, show));
        self
    }

    async fn cli(&self) -> AppResult<&WingetCli> {
        self.cli
            .get_or_try_init(|| async {
                let cli = match &self.location {
                    Location::Discover => WingetCli::connect(self.options.clone()).await?,
                    Location::Path(p) => {
                        WingetCli::connect_at(p.clone(), self.options.clone()).await?
                    }
                };
                Ok(match self.limits {
                    Some((search, show)) => cli.with_read_limits(search, show),
                    None => cli,
                })
            })
            .await
    }
}

impl Default for WingetCliSource {
    fn default() -> Self {
        Self::new()
    }
}

fn packaging(installer_type: Option<&str>) -> PackagingKind {
    match installer_type.map(str::to_ascii_lowercase).as_deref() {
        Some("msix") => PackagingKind::Msix,
        Some("msi" | "wix") => PackagingKind::Msi,
        Some("exe" | "nullsoft" | "inno" | "burn") => PackagingKind::Exe,
        Some("portable") => PackagingKind::Portable,
        _ => PackagingKind::Unknown,
    }
}

/// A search row has no publisher or description; the detail fills them later.
fn summary_from_row(row: SearchRow) -> PackageSummary {
    PackageSummary {
        id: PackageId::new(SourceKind::WingetCommunity, &row.id),
        name: row.name,
        publisher: String::new(),
        latest_version: row.version,
        description: String::new(),
        icon_url: None,
        rating: None,
        rating_count: None,
        source: SourceKind::WingetCommunity,
        is_paid: false,
        packaging: PackagingKind::Unknown,
    }
}

fn detail_from_block(block: ShowBlock) -> PackageDetail {
    let install_type = packaging(block.installer_type.as_deref());
    let full_description = block.description.clone().unwrap_or_default();
    let description: String = full_description
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(SUMMARY_DESCRIPTION_CHARS)
        .collect();
    let version = block.version.clone().unwrap_or_default();
    let mut versions = BTreeMap::new();
    if !version.is_empty() {
        versions.insert(
            version.clone(),
            PackageVersion {
                version: version.clone(),
                release_date: block.release_date.clone(),
                installer_url: block.installer_url.clone(),
                sha256: block.installer_sha256.clone(),
            },
        );
    }
    PackageDetail {
        summary: PackageSummary {
            id: PackageId::new(SourceKind::WingetCommunity, &block.id),
            name: block.name,
            publisher: block.publisher.unwrap_or_default(),
            latest_version: version,
            description,
            icon_url: None,
            rating: None,
            rating_count: None,
            source: SourceKind::WingetCommunity,
            is_paid: false,
            packaging: install_type,
        },
        full_description,
        screenshots: Vec::new(),
        website: block.homepage,
        license: block.license,
        license_url: block.license_url,
        release_notes: block.release_notes,
        installer: InstallerMetadata {
            architecture: None,
            install_type,
            size_bytes: None,
            sha256: block.installer_sha256,
        },
        versions,
    }
}

impl CatalogSource for WingetCliSource {
    fn kind(&self) -> SourceKind {
        SourceKind::WingetCommunity
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
        query: &'a SearchQuery,
    ) -> BoxFuture<'a, AppResult<Vec<PackageSummary>>> {
        Box::pin(async move {
            let text = query.text.trim();
            if text.is_empty() {
                return Ok(Vec::new());
            }
            let rows = self
                .cli()
                .await?
                .search_catalog(text, query.fetch_limit())
                .await?;
            Ok(rows.into_iter().map(summary_from_row).collect())
        })
    }

    fn get_detail<'a>(&'a self, package_id: &'a str) -> BoxFuture<'a, AppResult<PackageDetail>> {
        Box::pin(async move {
            let id = package_id.trim();
            if id.is_empty() {
                return Err(AppError::PackageNotFound(package_id.to_string()));
            }
            let block = self.cli().await?.show_package(id).await?;
            Ok(detail_from_block(block))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::winget_parse::parse_show;

    #[test]
    fn row_maps_to_a_summary_with_empty_publisher() {
        let s = summary_from_row(SearchRow {
            name: "VLC media player".into(),
            id: "VideoLAN.VLC".into(),
            version: "3.0.24".into(),
            extra: vec![],
        });
        assert_eq!(s.id.as_str(), "winget_community/VideoLAN.VLC");
        assert_eq!(s.publisher, "");
        assert_eq!(s.latest_version, "3.0.24");
        assert_eq!(s.packaging, PackagingKind::Unknown);
        assert!(!s.is_paid);
    }

    #[test]
    fn block_maps_to_a_detail() {
        let text = include_str!("../../tests/fixtures/winget/1.29/show-vlc.txt");
        let d = detail_from_block(parse_show(text, "VideoLAN.VLC").unwrap());
        assert_eq!(d.summary.publisher, "VideoLAN");
        assert!(d.summary.description.starts_with("VLC is a free"));
        assert!(d.summary.description.chars().count() <= SUMMARY_DESCRIPTION_CHARS);
        assert!(d.full_description.contains('\n'));
        assert_eq!(d.versions.len(), 1);
        assert_eq!(
            d.versions["3.0.24"].release_date.as_deref(),
            Some("2026-09-21")
        );
        assert_eq!(d.installer.install_type, PackagingKind::Msi);
    }

    #[test]
    fn installer_types_map() {
        assert_eq!(packaging(Some("MSIX")), PackagingKind::Msix);
        assert_eq!(packaging(Some("portable")), PackagingKind::Portable);
        for exe in ["nullsoft", "inno", "burn", "Nullsoft"] {
            assert_eq!(packaging(Some(exe)), PackagingKind::Exe, "{exe}");
        }
        assert_eq!(packaging(Some("wix")), PackagingKind::Msi);
        assert_eq!(packaging(Some("zip")), PackagingKind::Unknown);
        assert_eq!(packaging(None), PackagingKind::Unknown);
    }
}
