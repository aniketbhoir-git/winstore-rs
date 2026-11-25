//! Export of the installed packages to winget's JSON format (hardening design §4, "Export").
//!
//! `winget export` runs through the executor into a temp file. When the file parses, it is
//! copied to the exports folder as `store-export-<date>.json`. When it does not, nothing stays
//! on disk and the call returns `winget_failed`. No agreement flag, no network call of our own.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::UserDirs;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::executor::PackageExecutor;
use crate::services::import::{parse_import, read_limited};

/// What the UI shows after an export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportResult {
    pub path: String,
    pub file_name: String,
    pub package_count: usize,
}

/// `Documents\Store\exports` (from `directories`).
pub fn exports_dir() -> AppResult<PathBuf> {
    let docs = UserDirs::new()
        .and_then(|d| d.document_dir().map(Path::to_path_buf))
        .ok_or_else(|| AppError::InvalidConfig("cannot resolve the Documents folder".into()))?;
    Ok(docs.join("Store").join("exports"))
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    date_from_unix(secs)
}

fn date_from_unix(secs: u64) -> String {
    // Civil-from-days (Howard Hinnant), proleptic Gregorian.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Counts the packages of an export file with the import parser, or says why it is not one.
fn check_export(bytes: &[u8]) -> Result<usize, String> {
    parse_import(bytes)
        .map(|p| p.packages.len())
        .map_err(|e| match e {
            AppError::InvalidConfig(m) => format!("export {m}"),
            other => other.to_string(),
        })
}

fn bad_file(reason: String) -> AppError {
    AppError::WingetFailed {
        code: 0,
        stderr: reason,
    }
}

/// Runs the export and keeps the result in `dir` as `store-export-<date>.json`.
pub async fn export_installed(
    executor: &dyn PackageExecutor,
    dir: &Path,
    date: &str,
) -> AppResult<ExportResult> {
    let temp = std::env::temp_dir().join(format!(
        "store-export-{}-{}.json",
        std::process::id(),
        unique()
    ));
    export_with_temp(executor, &temp, dir, date).await
}

/// `export_installed` with the temp file chosen by the caller (a test seam). The temp file is
/// removed on every path, success or failure.
pub async fn export_with_temp(
    executor: &dyn PackageExecutor,
    temp: &Path,
    dir: &Path,
    date: &str,
) -> AppResult<ExportResult> {
    let result = run(executor, temp, dir, date).await;
    let _ = std::fs::remove_file(temp);
    result
}

async fn run(
    executor: &dyn PackageExecutor,
    temp: &Path,
    dir: &Path,
    date: &str,
) -> AppResult<ExportResult> {
    executor.export_installed(temp).await?;
    // `read_limited` checks `metadata().len()` before it reads.
    let bytes = match read_limited(temp) {
        Ok(b) => b,
        Err(AppError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(bad_file("winget wrote no export file".into()))
        }
        Err(AppError::InvalidConfig(m)) => return Err(bad_file(format!("export {m}"))),
        Err(e) => return Err(e),
    };
    let package_count = check_export(&bytes).map_err(bad_file)?;
    std::fs::create_dir_all(dir)?;
    let file_name = format!("store-export-{date}.json");
    let target = dir.join(&file_name);
    write_atomic(dir, &target, &bytes)?;
    Ok(ExportResult {
        path: target.display().to_string(),
        file_name,
        package_count,
    })
}

/// A number that differs on every call in this process, for temp file names.
fn unique() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::from(d.subsec_nanos()))
        .unwrap_or(0);
    nanos ^ (NEXT.fetch_add(1, Ordering::Relaxed) << 32)
}

/// Writes `bytes` to a temp file in the target folder, then renames it over `target`, so a
/// reader never sees a half-written file. The temp file does not end in `.json`.
fn write_atomic(dir: &Path, target: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = dir.join(format!(
        ".store-export-{}-{}.tmp",
        std::process::id(),
        unique()
    ));
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, target));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(AppError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_is_civil() {
        assert_eq!(date_from_unix(0), "1970-01-01");
        assert_eq!(date_from_unix(951_782_400), "2000-02-29");
        assert_eq!(date_from_unix(1_795_564_800), "2026-11-25");
    }

    #[test]
    fn check_counts_and_refuses() {
        let ok = br#"{"Sources":[{"Packages":[{"PackageIdentifier":"A.B"},{"PackageIdentifier":"C.D"}]}]}"#;
        assert_eq!(check_export(ok), Ok(2));
        let bom = [b"\xEF\xBB\xBF".as_slice(), ok.as_slice()].concat();
        assert_eq!(check_export(&bom), Ok(2));
        assert!(check_export(b"not json").is_err());
        assert!(check_export(br#"{"Sources":[{}]}"#).is_err());
        assert!(
            check_export(br#"{"Sources":[{"Packages":[{"PackageIdentifier":" "}]}]}"#).is_err()
        );
        assert!(check_export(&vec![b' '; 1024 * 1024 + 1]).is_err());
    }
}
