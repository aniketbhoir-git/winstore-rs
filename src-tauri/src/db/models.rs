//! Row structs for the SQLite tables and their conversions to domain types.
//!
//! Identifier convention: `packages.package_id` is the bare, source-local id
//! (the part after `source/`), keyed together with `packages.source`. In
//! `installs.package_id` and `operation_log.package_id` the column holds the
//! full `PackageId` string (`source/pkg`).
//!
//! Booleans are stored as `i64` (0/1). Malformed enum text or JSON read from
//! the database is reported as `AppError::Sql(sqlx::Error::Decode(..))`.

use std::str::FromStr;

use crate::domain::{
    InstallRecord, InstallState, LogLevel, Operation, OperationLogEntry, PackageDetail, PackageId,
    PackageSummary, PackagingKind, SourceConfig, SourceKind,
};
use crate::error::{AppError, AppResult};

fn decode<E: std::error::Error + Send + Sync + 'static>(e: E) -> AppError {
    AppError::Sql(sqlx::Error::Decode(Box::new(e)))
}

fn parse_enum<T>(s: &str) -> AppResult<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    s.parse::<T>().map_err(decode)
}

fn parse_package_id(s: &str) -> AppResult<PackageId> {
    let (source, rest) = PackageId::parse(s).map_err(|e| decode(StringError(e.to_string())))?;
    Ok(PackageId::new(source, &rest))
}

#[derive(Debug)]
struct StringError(String);

impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StringError {}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PackageRow {
    pub source: String,
    pub package_id: String,
    pub name: String,
    pub publisher: String,
    pub latest_version: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<i64>,
    pub is_paid: i64,
    pub packaging: String,
    pub detail_json: Option<String>,
    pub fetched_at: i64,
}

impl TryFrom<PackageRow> for PackageSummary {
    type Error = AppError;

    fn try_from(row: PackageRow) -> AppResult<Self> {
        let source: SourceKind = parse_enum(&row.source)?;
        Ok(PackageSummary {
            id: PackageId::new(source, &row.package_id),
            name: row.name,
            publisher: row.publisher,
            latest_version: row.latest_version,
            description: row.description,
            icon_url: row.icon_url,
            rating: row.rating.map(|r| r as f32),
            rating_count: row
                .rating_count
                .map(u32::try_from)
                .transpose()
                .map_err(decode)?,
            source,
            is_paid: row.is_paid != 0,
            packaging: parse_enum::<PackagingKind>(&row.packaging)?,
        })
    }
}

impl PackageRow {
    /// Parses the cached detail JSON, if any.
    pub fn into_detail(self) -> AppResult<Option<PackageDetail>> {
        self.detail_json
            .as_deref()
            .map(|j| serde_json::from_str::<PackageDetail>(j).map_err(decode))
            .transpose()
    }

    pub fn from_summary(
        s: &PackageSummary,
        detail: Option<&PackageDetail>,
        fetched_at: i64,
    ) -> AppResult<Self> {
        let (_, bare_id) = PackageId::parse(s.id.as_str())?;
        let detail_json = detail
            .map(|d| serde_json::to_string(d).map_err(decode))
            .transpose()?;
        Ok(PackageRow {
            source: s.source.to_string(),
            package_id: bare_id,
            name: s.name.clone(),
            publisher: s.publisher.clone(),
            latest_version: s.latest_version.clone(),
            description: s.description.clone(),
            icon_url: s.icon_url.clone(),
            rating: s.rating.map(f64::from),
            rating_count: s.rating_count.map(i64::from),
            is_paid: i64::from(s.is_paid),
            packaging: s.packaging.to_string(),
            detail_json,
            fetched_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct InstallRow {
    pub package_id: String,
    pub source: String,
    pub state: String,
    pub installed_version: Option<String>,
    pub target_version: Option<String>,
    pub pinned: i64,
    pub auto_update: i64,
    pub last_update_check: Option<i64>,
    pub last_updated: Option<i64>,
}

impl TryFrom<InstallRow> for InstallRecord {
    type Error = AppError;

    fn try_from(row: InstallRow) -> AppResult<Self> {
        Ok(InstallRecord {
            package_id: parse_package_id(&row.package_id)?,
            source: parse_enum::<SourceKind>(&row.source)?,
            state: parse_enum::<InstallState>(&row.state)?,
            installed_version: row.installed_version,
            target_version: row.target_version,
            pinned: row.pinned != 0,
            auto_update: row.auto_update != 0,
            last_update_check: row.last_update_check,
            last_updated: row.last_updated,
        })
    }
}

impl From<&InstallRecord> for InstallRow {
    fn from(r: &InstallRecord) -> Self {
        InstallRow {
            package_id: r.package_id.to_string(),
            source: r.source.to_string(),
            state: r.state.to_string(),
            installed_version: r.installed_version.clone(),
            target_version: r.target_version.clone(),
            pinned: i64::from(r.pinned),
            auto_update: i64::from(r.auto_update),
            last_update_check: r.last_update_check,
            last_updated: r.last_updated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct SourceRow {
    pub kind: String,
    pub enabled: i64,
    pub priority: i64,
    pub display_name: String,
    pub extra_config: Option<String>,
    pub last_sync: Option<i64>,
}

impl TryFrom<SourceRow> for SourceConfig {
    type Error = AppError;

    fn try_from(row: SourceRow) -> AppResult<Self> {
        let extra_config = match row.extra_config.as_deref() {
            None => serde_json::Value::Null,
            Some(j) => serde_json::from_str(j).map_err(decode)?,
        };
        Ok(SourceConfig {
            kind: parse_enum::<SourceKind>(&row.kind)?,
            enabled: row.enabled != 0,
            priority: i32::try_from(row.priority).map_err(decode)?,
            display_name: row.display_name,
            extra_config,
            last_sync: row.last_sync,
        })
    }
}

impl From<&SourceConfig> for SourceRow {
    fn from(c: &SourceConfig) -> Self {
        SourceRow {
            kind: c.kind.to_string(),
            enabled: i64::from(c.enabled),
            priority: i64::from(c.priority),
            display_name: c.display_name.clone(),
            extra_config: if c.extra_config.is_null() {
                None
            } else {
                Some(c.extra_config.to_string())
            },
            last_sync: c.last_sync,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct OperationLogRow {
    pub id: Option<i64>,
    pub ts: i64,
    pub level: String,
    pub operation: Option<String>,
    pub package_id: Option<String>,
    pub message: String,
    pub detail: Option<String>,
}

impl TryFrom<OperationLogRow> for OperationLogEntry {
    type Error = AppError;

    fn try_from(row: OperationLogRow) -> AppResult<Self> {
        Ok(OperationLogEntry {
            id: row.id,
            ts: row.ts,
            level: parse_enum::<LogLevel>(&row.level)?,
            operation: row
                .operation
                .as_deref()
                .map(parse_enum::<Operation>)
                .transpose()?,
            package_id: row
                .package_id
                .as_deref()
                .map(parse_package_id)
                .transpose()?,
            message: row.message,
            detail: row.detail,
        })
    }
}

impl From<&OperationLogEntry> for OperationLogRow {
    fn from(e: &OperationLogEntry) -> Self {
        OperationLogRow {
            id: e.id,
            ts: e.ts,
            level: e.level.to_string(),
            operation: e.operation.map(|o| o.to_string()),
            package_id: e.package_id.as_ref().map(|p| p.to_string()),
            message: e.message.clone(),
            detail: e.detail.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::fixtures;
    use serde_json::json;

    #[test]
    fn summary_round_trip() {
        let s = fixtures::summary(SourceKind::MsStore, "9NBLGGH4NNS1", "Name");
        let row = PackageRow::from_summary(&s, None, 42).unwrap();
        assert_eq!(row.package_id, "9NBLGGH4NNS1");
        assert_eq!(row.source, "ms_store");
        assert_eq!(row.fetched_at, 42);
        assert!(row.detail_json.is_none());
        assert_eq!(PackageSummary::try_from(row.clone()).unwrap(), s);
        assert!(row.into_detail().unwrap().is_none());

        let mut bare = s.clone();
        bare.icon_url = None;
        bare.rating = None;
        bare.rating_count = None;
        bare.is_paid = true;
        let row = PackageRow::from_summary(&bare, None, 1).unwrap();
        assert_eq!(PackageSummary::try_from(row).unwrap(), bare);
    }

    #[test]
    fn detail_round_trip() {
        let d = fixtures::detail(SourceKind::Chocolatey, "tool", "Tool");
        let row = PackageRow::from_summary(&d.summary, Some(&d), 7).unwrap();
        assert!(row.detail_json.is_some());
        assert_eq!(PackageSummary::try_from(row.clone()).unwrap(), d.summary);
        let back = row.into_detail().unwrap().unwrap();
        assert_eq!(back, d);
        assert_eq!(back.versions.len(), 2);
        assert_eq!(back.screenshots.len(), 2);
    }

    #[test]
    fn bad_package_row_is_sql_error() {
        let s = fixtures::summary(SourceKind::Scoop, "p", "P");
        let mut row = PackageRow::from_summary(&s, None, 1).unwrap();
        row.packaging = "weird".into();
        assert_eq!(
            PackageSummary::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.packaging = "msi".into();
        row.source = "weird".into();
        assert_eq!(
            PackageSummary::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.source = "scoop".into();
        row.detail_json = Some("{not json".into());
        assert_eq!(row.into_detail().unwrap_err().code(), "sql");
    }

    #[test]
    fn install_round_trip() {
        let id = PackageId::new(SourceKind::WingetCommunity, "A.B");
        let none = InstallRecord::new(id.clone(), SourceKind::WingetCommunity);
        assert_eq!(
            InstallRecord::try_from(InstallRow::from(&none)).unwrap(),
            none
        );

        let some = InstallRecord {
            package_id: id,
            source: SourceKind::WingetCommunity,
            state: InstallState::UpdateAvailable,
            installed_version: Some("1.0".into()),
            target_version: Some("2.0".into()),
            pinned: true,
            auto_update: false,
            last_update_check: Some(10),
            last_updated: Some(20),
        };
        let row = InstallRow::from(&some);
        assert_eq!(row.package_id, "winget_community/A.B");
        assert_eq!(row.state, "update_available");
        assert_eq!(InstallRecord::try_from(row).unwrap(), some);
    }

    #[test]
    fn bad_install_row_is_sql_error() {
        let rec = InstallRecord::new(PackageId::new(SourceKind::Scoop, "x"), SourceKind::Scoop);
        let mut row = InstallRow::from(&rec);
        row.state = "weird".into();
        assert_eq!(
            InstallRecord::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.state = "installed".into();
        row.package_id = "no-slash".into();
        assert_eq!(InstallRecord::try_from(row).unwrap_err().code(), "sql");
    }

    #[test]
    fn source_round_trip() {
        let plain = SourceConfig::default_for(SourceKind::Scoop);
        let row = SourceRow::from(&plain);
        assert!(row.extra_config.is_none());
        assert_eq!(SourceConfig::try_from(row).unwrap(), plain);

        let mut rich = SourceConfig::default_for(SourceKind::MsStore);
        rich.extra_config = json!({"market": "US", "n": [1, 2], "nested": {"a": null}});
        rich.last_sync = Some(99);
        rich.priority = -3;
        let row = SourceRow::from(&rich);
        assert_eq!(SourceConfig::try_from(row).unwrap(), rich);
    }

    #[test]
    fn bad_source_row_is_sql_error() {
        let mut row = SourceRow::from(&SourceConfig::default_for(SourceKind::Scoop));
        row.kind = "weird".into();
        assert_eq!(
            SourceConfig::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.kind = "scoop".into();
        row.extra_config = Some("{bad".into());
        assert_eq!(
            SourceConfig::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.extra_config = None;
        row.priority = i64::MAX;
        assert_eq!(SourceConfig::try_from(row).unwrap_err().code(), "sql");
    }

    #[test]
    fn log_round_trip() {
        let full = OperationLogEntry {
            id: Some(5),
            ts: 100,
            level: LogLevel::Warn,
            operation: Some(Operation::Update),
            package_id: Some(PackageId::new(SourceKind::Chocolatey, "x")),
            message: "m".into(),
            detail: Some("d".into()),
        };
        assert_eq!(
            OperationLogEntry::try_from(OperationLogRow::from(&full)).unwrap(),
            full
        );
        let min = OperationLogEntry {
            id: None,
            ts: 1,
            level: LogLevel::Debug,
            operation: None,
            package_id: None,
            message: "m".into(),
            detail: None,
        };
        assert_eq!(
            OperationLogEntry::try_from(OperationLogRow::from(&min)).unwrap(),
            min
        );
    }

    #[test]
    fn bad_log_row_is_sql_error() {
        let mut row = OperationLogRow {
            id: None,
            ts: 1,
            level: "loud".into(),
            operation: None,
            package_id: None,
            message: "m".into(),
            detail: None,
        };
        assert_eq!(
            OperationLogEntry::try_from(row.clone()).unwrap_err().code(),
            "sql"
        );
        row.level = "info".into();
        row.operation = Some("explode".into());
        assert_eq!(OperationLogEntry::try_from(row).unwrap_err().code(), "sql");
    }
}
