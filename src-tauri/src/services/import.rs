//! Import: reads a winget export file and builds a review list (hardening design §4,
//! "Import: the file"). Plan step 6: this module reads and checks; it installs nothing and
//! starts no process. The export check of `services/export.rs` uses the same parser.
//!
//! The file is untrusted input. A path is allowed only when the core recorded it from its own
//! drop event (`ImportPaths`) or when it is a `.json` file in the exports folder.

use std::collections::{HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::domain::validate_winget_id;
use crate::error::{AppError, AppResult};

/// Largest import file (1 MiB).
pub const MAX_IMPORT_BYTES: u64 = 1024 * 1024;
/// Most packages in one file.
pub const MAX_IMPORT_PACKAGES: usize = 500;
/// Most dropped paths that the core remembers; the oldest entry goes first.
pub const MAX_DROPPED_PATHS: usize = 16;

fn invalid(msg: impl Into<String>) -> AppError {
    AppError::InvalidConfig(msg.into())
}

// A strict subset of the schema in `microsoft/winget-cli` (`schemas/JSON/packages`): only the
// fields that Store reads. Other fields (`$schema`, `CreationDate`, `WinGetVersion`) are ignored.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawFile {
    sources: Vec<RawSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawSource {
    #[serde(default)]
    source_details: Option<RawDetails>,
    packages: Vec<RawPackage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawDetails {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawPackage {
    package_identifier: String,
    #[serde(default)]
    version: Option<String>,
}

/// One package entry of a file, before the id is validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPackage {
    pub id: String,
    pub version: Option<String>,
    /// `SourceDetails.Name` of its source; empty when the file has none.
    pub source: String,
}

/// A file that parsed. The ids are not validated yet (`build_review` does that).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedImport {
    pub packages: Vec<ImportPackage>,
}

/// Parses an export file: UTF-8 (a BOM is allowed), at most 1 MiB and 500 packages, no empty id.
/// Any failure is `invalid_config`.
pub fn parse_import(bytes: &[u8]) -> AppResult<ParsedImport> {
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(invalid("the file is larger than 1 MiB"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("the file is not UTF-8"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let file: RawFile =
        serde_json::from_str(text).map_err(|e| invalid(format!("the file is not valid: {e}")))?;
    let mut packages = Vec::new();
    for source in file.sources {
        let name = source
            .source_details
            .and_then(|d| d.name)
            .unwrap_or_default();
        for p in source.packages {
            if p.package_identifier.trim().is_empty() {
                return Err(invalid("the file holds an empty package id"));
            }
            packages.push(ImportPackage {
                id: p.package_identifier,
                version: p.version,
                source: name.clone(),
            });
            if packages.len() > MAX_IMPORT_PACKAGES {
                return Err(invalid("the file holds more than 500 packages"));
            }
        }
    }
    Ok(ParsedImport { packages })
}

/// Reads a file of at most `MAX_IMPORT_BYTES`. The size is checked before the read, and the
/// read itself is capped, so a file that grows in between is not read whole.
pub fn read_limited(path: &Path) -> AppResult<Vec<u8>> {
    read_limited_file(std::fs::File::open(path)?)
}

/// `read_limited` on a file that is already open: the size and the bytes come from the same
/// handle.
pub fn read_limited_file(file: std::fs::File) -> AppResult<Vec<u8>> {
    if file.metadata()?.len() > MAX_IMPORT_BYTES {
        return Err(invalid("the file is larger than 1 MiB"));
    }
    let mut buf = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_IMPORT_BYTES {
        return Err(invalid("the file is larger than 1 MiB"));
    }
    Ok(buf)
}

/// Pure string check, run before any filesystem call on a path from the webview. It passes
/// only a string of the form `<letter>:`, then `\` or `/`, then a first character that is not
/// a separator (`C:\dir\file`, `C:/dir/file`), with no further `:` and no NUL. So a UNC path
/// (`\\host\share`), a device path (`\\.\`, `\\?\`), a relative or drive-relative path
/// (`C:file`), an alternate data stream and a NUL all fail. Resolving such a path can open a
/// network or pipe connection (NTLM hash leak), so the core never calls `canonicalize` on it.
/// It does not look at the drive type or at device names; see `has_reserved_device_name`.
pub fn is_local_drive_path(requested: &str) -> bool {
    let b = requested.as_bytes();
    let sep = |c: u8| c == b'\\' || c == b'/';
    b.len() >= 4
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && sep(b[2])
        && !sep(b[3])
        && !requested[2..].contains([':', '\0'])
}

/// Pure string check: true when any component of `requested` is a Windows reserved device name
/// (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM0-9`, `LPT0-9` and the superscript
/// forms `COM¹²³`, `LPT¹²³`). Case-insensitive. A trailing dot or space and any extension
/// (`NUL.json`, `nul .txt`, `NUL...`) do not hide the name: Windows resolves them to the device.
pub fn has_reserved_device_name(requested: &str) -> bool {
    requested.split(['\\', '/']).any(|comp| {
        let stem = comp.split('.').next().unwrap_or("").trim_end_matches(' ');
        let up = stem.to_uppercase();
        match up.as_str() {
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
            _ => {
                let rest = up.strip_prefix("COM").or_else(|| up.strip_prefix("LPT"));
                let mut it = rest.into_iter().flat_map(str::chars);
                matches!(
                    (it.next(), it.next()),
                    (Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
                )
            }
        }
    })
}

/// Asks the OS whether the drive of `requested` (a string that passed `is_local_drive_path`)
/// is a network drive. A drive type call does not open the path. On any other platform: false.
#[cfg(windows)]
fn drive_is_remote(requested: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    /// `DRIVE_REMOTE` from `fileapi.h`; the `windows` crate keeps it in another feature.
    const DRIVE_REMOTE: u32 = 4;
    let Some(letter) = requested.chars().next() else {
        return false;
    };
    let root: Vec<u16> = format!("{letter}:\\").encode_utf16().chain([0]).collect();
    // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
    unsafe { GetDriveTypeW(windows::core::PCWSTR(root.as_ptr())) == DRIVE_REMOTE }
}

#[cfg(not(windows))]
fn drive_is_remote(_requested: &str) -> bool {
    false
}

/// Opens `canon` (already canonical) once and checks the handle, not the path: the open does
/// not follow a link in the last component (`FILE_FLAG_OPEN_REPARSE_POINT`), and a reparse
/// point or a non-file is refused. This closes the swap of the file for a link between the
/// path check and the open. A link swapped into a parent folder in that gap is not caught.
fn open_checked(canon: &Path) -> AppResult<std::fs::File> {
    let denied = || invalid("this file cannot be read here; drop it on the window instead");
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = opts.open(canon).map_err(|_| denied())?;
    let meta = file.metadata().map_err(|_| denied())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(denied()); // FILE_ATTRIBUTE_REPARSE_POINT
        }
    }
    if !meta.is_file() {
        return Err(denied());
    }
    Ok(file)
}

/// Why a row can or cannot be ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    /// The only status that can be ticked.
    Ready,
    /// The ledger or `winget list` shows it as installed.
    Installed,
    /// The id failed `validate_winget_id()`.
    NotValid,
    /// A winget source other than `winget`.
    SourceNotSupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportRow {
    pub package_id: String,
    /// Shown only; an install takes the latest version.
    pub version: Option<String>,
    pub source: String,
    pub status: RowStatus,
}

/// What `read_import_file` returns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReview {
    pub file_name: String,
    pub rows: Vec<ImportRow>,
    /// False when the installed packages could not be read; no row is then "installed".
    pub installed_known: bool,
}

/// Builds the review list. `installed` holds lower-case native ids. Rows with the same id and
/// source (case-insensitive) are merged into the first one.
pub fn build_review(
    file_name: &str,
    parsed: &ParsedImport,
    installed: &HashSet<String>,
    installed_known: bool,
) -> ImportReview {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for p in &parsed.packages {
        let key = (p.id.to_lowercase(), p.source.to_lowercase());
        if !seen.insert(key) {
            continue;
        }
        let status = if !p.source.eq_ignore_ascii_case("winget") {
            RowStatus::SourceNotSupported
        } else if validate_winget_id(&p.id).is_err() {
            RowStatus::NotValid
        } else if installed.contains(&p.id.to_lowercase()) {
            RowStatus::Installed
        } else {
            RowStatus::Ready
        };
        rows.push(ImportRow {
            package_id: p.id.clone(),
            version: p.version.clone(),
            source: p.source.clone(),
            status,
        });
    }
    ImportReview {
        file_name: file_name.to_string(),
        rows,
        installed_known,
    }
}

/// The canonical paths of files that the user dropped on the window, in memory only.
#[derive(Debug, Default)]
pub struct ImportPaths {
    dropped: Mutex<VecDeque<PathBuf>>,
}

impl ImportPaths {
    /// Records one dropped path: canonicalised, a regular file only. Returns the canonical
    /// path, or `None` for anything else (a directory, a missing path).
    pub fn record_drop(&self, path: &Path) -> Option<PathBuf> {
        let canon = std::fs::canonicalize(path).ok()?;
        if !std::fs::metadata(&canon).ok()?.is_file() {
            return None;
        }
        let mut set = self.dropped.lock().unwrap_or_else(|e| e.into_inner());
        set.retain(|p| p != &canon);
        set.push_back(canon.clone());
        while set.len() > MAX_DROPPED_PATHS {
            set.pop_front();
        }
        Some(canon)
    }

    /// Refuses a non-local-drive string, a device name and a network drive, then canonicalises
    /// `requested` and allows it when it was dropped, or when it is a `.json`
    /// file whose parent is the canonical exports folder. Otherwise `invalid_config`, and the
    /// caller does not open the file.
    pub fn resolve(&self, requested: &str, exports_dir: &Path) -> AppResult<PathBuf> {
        self.resolve_with(requested, exports_dir, drive_is_remote)
    }

    /// `resolve` with the network-drive check passed in, so a test needs no network drive.
    fn resolve_with(
        &self,
        requested: &str,
        exports_dir: &Path,
        is_remote: fn(&str) -> bool,
    ) -> AppResult<PathBuf> {
        let denied = || invalid("this file cannot be read here; drop it on the window instead");
        // A recorded drop path (canonical, often `\?\C:\...`) matches by string, no disk call.
        let recorded = self
            .dropped
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|p| p.as_os_str() == std::ffi::OsStr::new(requested))
            .cloned();
        if let Some(p) = recorded {
            return Ok(p);
        }
        // Before any filesystem call: `canonicalize` on a UNC or device path connects out.
        if !is_local_drive_path(requested)
            || has_reserved_device_name(requested)
            || is_remote(requested)
        {
            return Err(denied());
        }
        let canon = std::fs::canonicalize(requested).map_err(|_| denied())?;
        if !std::fs::metadata(&canon).map_err(|_| denied())?.is_file() {
            return Err(denied());
        }
        let dropped = self
            .dropped
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&canon);
        if dropped {
            return Ok(canon);
        }
        let dir = std::fs::canonicalize(exports_dir).map_err(|_| denied())?;
        let is_json = canon
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if is_json && canon.parent() == Some(dir.as_path()) {
            return Ok(canon);
        }
        Err(denied())
    }
}

impl ImportPaths {
    /// `resolve`, then one open of the canonical path with a checked handle (`open_checked`),
    /// then the capped read from that same handle. Returns the display name and the bytes.
    pub fn read(&self, requested: &str, exports_dir: &Path) -> AppResult<(String, Vec<u8>)> {
        let canon = self.resolve(requested, exports_dir)?;
        let bytes = read_limited_file(open_checked(&canon)?)?;
        Ok((display_name(&canon), bytes))
    }
}

/// File name for display, never the full path.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"$schema":"x","Sources":[{"Packages":[
        {"PackageIdentifier":"VideoLAN.VLC","Version":"3.0.21"},
        {"PackageIdentifier":"Git.Git"}],
        "SourceDetails":{"Name":"winget","Type":"Microsoft.PreIndexed.Package"}}]}"#;

    #[test]
    fn parses_a_recorded_file_with_and_without_version() {
        let p = parse_import(FILE.as_bytes()).unwrap();
        assert_eq!(p.packages.len(), 2);
        assert_eq!(p.packages[0].version.as_deref(), Some("3.0.21"));
        assert_eq!(p.packages[1].version, None);
        assert_eq!(p.packages[1].source, "winget");
        let fixture = include_bytes!("../../tests/fixtures/winget/1.29/export-small.json");
        assert_eq!(parse_import(fixture).unwrap().packages.len(), 3);
    }

    #[test]
    fn accepts_a_bom_and_refuses_bad_files() {
        let bom = [b"\xEF\xBB\xBF".as_slice(), FILE.as_bytes()].concat();
        assert!(parse_import(&bom).is_ok());
        assert!(parse_import(b"not json").is_err());
        assert!(parse_import(&[0xff, 0xfe]).is_err());
        assert!(parse_import(br#"{"Sources":[{}]}"#).is_err());
        assert!(parse_import(br#"{}"#).is_err());
        let empty = br#"{"Sources":[{"Packages":[{"PackageIdentifier":" "}]}]}"#;
        assert!(parse_import(empty).is_err());
    }

    #[test]
    fn limits_are_size_and_count() {
        let pkgs = |n: usize| {
            let one = r#"{"PackageIdentifier":"A.B"}"#;
            format!(
                r#"{{"Sources":[{{"Packages":[{}]}}]}}"#,
                vec![one; n].join(",")
            )
        };
        assert_eq!(
            parse_import(pkgs(500).as_bytes()).unwrap().packages.len(),
            500
        );
        let err = parse_import(pkgs(501).as_bytes()).unwrap_err();
        assert!(matches!(err, AppError::InvalidConfig(_)));
        let mut big = FILE.as_bytes().to_vec();
        big.resize(MAX_IMPORT_BYTES as usize, b' ');
        assert!(parse_import(&big).is_ok());
        big.push(b' ');
        assert!(matches!(
            parse_import(&big),
            Err(AppError::InvalidConfig(_))
        ));
    }

    #[test]
    fn review_marks_rows_and_merges_duplicates() {
        let json = r#"{"Sources":[
          {"SourceDetails":{"Name":"winget"},"Packages":[
            {"PackageIdentifier":"VideoLAN.VLC","Version":"3.0.21"},
            {"PackageIdentifier":"videolan.vlc"},
            {"PackageIdentifier":"Git.Git"},
            {"PackageIdentifier":"--accept-package-agreements"},
            {"PackageIdentifier":"VLC"}]},
          {"SourceDetails":{"Name":"msstore"},"Packages":[{"PackageIdentifier":"9NBLGGH4NNS1"}]},
          {"Packages":[{"PackageIdentifier":"No.Source"}]}]}"#;
        let parsed = parse_import(json.as_bytes()).unwrap();
        let installed = HashSet::from(["git.git".to_string()]);
        let r = build_review("f.json", &parsed, &installed, true);
        let status: Vec<_> = r
            .rows
            .iter()
            .map(|x| (x.package_id.as_str(), x.status))
            .collect();
        assert_eq!(
            status,
            vec![
                ("VideoLAN.VLC", RowStatus::Ready),
                ("Git.Git", RowStatus::Installed),
                ("--accept-package-agreements", RowStatus::NotValid),
                ("VLC", RowStatus::NotValid),
                ("9NBLGGH4NNS1", RowStatus::SourceNotSupported),
                ("No.Source", RowStatus::SourceNotSupported),
            ]
        );
        assert_eq!(r.rows[0].version.as_deref(), Some("3.0.21"));
    }

    struct Fx {
        dir: tempfile::TempDir,
        exports: PathBuf,
        paths: ImportPaths,
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let exports = dir.path().join("exports");
        std::fs::create_dir(&exports).unwrap();
        Fx {
            dir,
            exports,
            paths: ImportPaths::default(),
        }
    }

    fn put(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, FILE).unwrap();
        p
    }

    #[test]
    fn a_dropped_path_opens_and_an_unknown_one_does_not() {
        let fx = fx();
        let outside = put(fx.dir.path(), "dropped.txt");
        let other = put(fx.dir.path(), "other.json");
        let s = |p: &Path| p.display().to_string();
        assert!(fx.paths.resolve(&s(&outside), &fx.exports).is_err());
        assert!(fx.paths.record_drop(&outside).is_some());
        assert!(fx.paths.resolve(&s(&outside), &fx.exports).is_ok());
        // Not dropped and not in the exports folder.
        let err = fx.paths.resolve(&s(&other), &fx.exports).unwrap_err();
        assert!(matches!(err, AppError::InvalidConfig(_)));
    }

    #[test]
    fn only_a_local_drive_path_passes_the_string_check() {
        for ok in ["C:\\a\\b.json", "d:/a/b.json", "Z:\\x"] {
            assert!(is_local_drive_path(ok), "{ok}");
        }
        for bad in [
            "",
            "\\\\host\\share\\x.json",
            "//host/share/x.json",
            "\\\\?\\UNC\\host\\share\\x.json",
            "\\\\.\\pipe\\x",
            "\\\\?\\C:\\a\\b.json",
            "\\\\.\\C:\\a",
            "a.json",
            "..\\a.json",
            "\\a\\b.json",
            "C:a.json",
            "C:",
            "C:\\\\host\\x",
            "C:\\a\\b.json:stream",
            "C:\\a\0b.json",
            "1:\\a.json",
        ] {
            assert!(!is_local_drive_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn network_and_device_paths_fail_the_string_check() {
        // Pure string check: no `resolve` call, so no path here can reach the network.
        for bad in [
            "\\\\host\\share\\x.json",
            "\\\\?\\UNC\\host\\share\\x.json",
            "\\\\.\\pipe\\x",
            "\\\\?\\C:\\a\\x.json",
            "x.json",
            "..\\x.json",
        ] {
            assert!(!is_local_drive_path(bad), "{bad}");
        }
    }

    #[test]
    fn reserved_device_names_are_found_in_any_component() {
        for bad in [
            "C:\\NUL",
            "C:\\a\\nul.json",
            "C:\\a\\Con\\x.json",
            "C:/a/PRN.txt",
            "C:\\AUX.tar.gz",
            "C:\\com1.json",
            "C:\\COM0",
            "C:\\lpt9.json",
            "C:\\NUL.",
            "C:\\NUL ",
            "C:\\NUL .json",
            "C:\\NUL...json",
            "C:\\COM\u{b9}",
            "C:\\lpt\u{b2}.txt",
            "C:\\COM\u{b3}.json",
            "C:\\CONIN$",
            "C:\\CONOUT$.x",
        ] {
            assert!(has_reserved_device_name(bad), "{bad:?}");
        }
        for ok in [
            "C:\\a\\b.json",
            "C:\\console.json",
            "C:\\COM10.json",
            "C:\\COMX",
            "C:\\nulx\\a.json",
            "C:\\a.nul",
            "C:\\LPT",
        ] {
            assert!(!has_reserved_device_name(ok), "{ok:?}");
        }
    }

    #[test]
    fn resolve_refuses_device_names_and_remote_drives_before_the_disk() {
        let fx = fx();
        // Nothing here exists: only the string and drive checks can have refused these.
        let missing = fx.dir.path().join("none");
        for bad in ["C:\\a\\NUL.json", "C:\\COM1", "C:\\x\\lpt\u{b9}.json"] {
            let err = fx.paths.resolve(bad, &missing).unwrap_err();
            assert!(matches!(err, AppError::InvalidConfig(_)), "{bad}");
        }
        // A fake drive type that says "remote" refuses an otherwise fine local path.
        let ok = put(&fx.exports, "store-export-2026-11-25.json");
        let s = ok.display().to_string();
        assert!(fx.paths.resolve_with(&s, &fx.exports, |_| false).is_ok());
        let err = fx
            .paths
            .resolve_with(&s, &fx.exports, |_| true)
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidConfig(_)));
    }

    #[test]
    fn read_returns_the_file_name_and_bytes() {
        let fx = fx();
        let ok = put(&fx.exports, "store-export-2026-11-25.json");
        let (name, bytes) = fx
            .paths
            .read(&ok.display().to_string(), &fx.exports)
            .unwrap();
        assert_eq!(name, "store-export-2026-11-25.json");
        assert_eq!(bytes, FILE.as_bytes());
    }

    #[test]
    fn exports_folder_rules() {
        let fx = fx();
        let s = |p: &Path| p.display().to_string();
        let ok = put(&fx.exports, "store-export-2026-11-25.json");
        assert!(fx.paths.resolve(&s(&ok), &fx.exports).is_ok());
        let txt = put(&fx.exports, "notes.txt");
        assert!(fx.paths.resolve(&s(&txt), &fx.exports).is_err());
        // A `..` path that ends outside the folder.
        put(fx.dir.path(), "up.json");
        let up = fx.exports.join("..").join("up.json");
        assert!(fx.paths.resolve(&s(&up), &fx.exports).is_err());
        // A `..` path that stays inside is the same file.
        let inside = fx
            .exports
            .join("..")
            .join("exports")
            .join("store-export-2026-11-25.json");
        assert!(fx.paths.resolve(&s(&inside), &fx.exports).is_ok());
        // A directory, a missing file, an empty path.
        assert!(fx.paths.resolve(&s(&fx.exports), &fx.exports).is_err());
        assert!(fx
            .paths
            .resolve(&s(&fx.exports.join("gone.json")), &fx.exports)
            .is_err());
        assert!(fx.paths.resolve("", &fx.exports).is_err());
    }

    #[test]
    fn a_link_that_points_outside_is_refused() {
        let fx = fx();
        let target = put(fx.dir.path(), "target.json");
        let link = fx.exports.join("link.json");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        if made.is_err() {
            return; // Windows without the symlink right: nothing to test.
        }
        assert!(fx
            .paths
            .resolve(&link.display().to_string(), &fx.exports)
            .is_err());
    }

    #[test]
    fn the_17th_drop_removes_the_oldest() {
        let fx = fx();
        let files: Vec<_> = (0..=MAX_DROPPED_PATHS)
            .map(|i| put(fx.dir.path(), &format!("d{i}.txt")))
            .collect();
        for f in &files {
            assert!(fx.paths.record_drop(f).is_some());
        }
        let s = |p: &Path| p.display().to_string();
        assert!(fx.paths.resolve(&s(&files[0]), &fx.exports).is_err());
        assert!(fx.paths.resolve(&s(&files[1]), &fx.exports).is_ok());
        assert!(fx
            .paths
            .resolve(&s(&files[MAX_DROPPED_PATHS]), &fx.exports)
            .is_ok());
    }

    #[test]
    fn a_dropped_directory_is_not_recorded() {
        let fx = fx();
        assert!(fx.paths.record_drop(&fx.exports).is_none());
        assert!(fx.paths.record_drop(&fx.exports.join("gone")).is_none());
    }

    #[test]
    fn read_limited_checks_the_size_first() {
        let fx = fx();
        let p = put(fx.dir.path(), "a.json");
        assert!(read_limited(&p).is_ok());
        std::fs::write(&p, vec![b' '; MAX_IMPORT_BYTES as usize + 1]).unwrap();
        assert!(matches!(read_limited(&p), Err(AppError::InvalidConfig(_))));
    }
}
