//! Pure winget output parsing (no I/O, no process code).
//!
//! Facts and patterns come from `.claude/skills/winget-executor`. Patterns marked INFERRED there
//! (install/uninstall stdout) are unconfirmed until the manual smoke run.

use std::collections::VecDeque;
use std::fmt;

use crate::domain::OperationStage;
use crate::error::AppError;

/// Oldest winget this project supports (project choice; see the skill).
pub const MIN_VERSION: WingetVersion = WingetVersion {
    major: 1,
    minor: 6,
    patch: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WingetVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl WingetVersion {
    /// Parses `v1.29.380`, `1.6`, `v1.7.10861-preview`.
    pub fn parse(text: &str) -> Option<WingetVersion> {
        let word = text.split_whitespace().next()?;
        let word = word.trim_start_matches(['v', 'V']);
        let core = word.split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0,
        };
        Some(WingetVersion {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for WingetVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------------------------
// Line splitting
// ---------------------------------------------------------------------------------------------

const MAX_LINE_BYTES: usize = 64 * 1024;

/// Splits a byte stream on `\r` and `\n` (winget redraws lines with a bare `\r`), decodes each
/// line lossily and drops empty lines. Split points are ASCII, so UTF-8 sequences stay whole.
#[derive(Debug, Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == b'\r' || b == b'\n' {
                self.flush_into(&mut out);
            } else {
                self.buf.push(b);
                if self.buf.len() >= MAX_LINE_BYTES {
                    self.flush_into(&mut out);
                }
            }
        }
        out
    }

    /// Returns the unterminated last line, if any.
    pub fn finish(&mut self) -> Option<String> {
        let mut out = Vec::new();
        self.flush_into(&mut out);
        out.pop()
    }

    fn flush_into(&mut self, out: &mut Vec<String>) {
        if self.buf.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(&self.buf).trim_end().to_string();
        self.buf.clear();
        if !line.trim().is_empty() {
            out.push(line);
        }
    }
}

/// Single-character redraw frames (`- \ | /`).
pub fn is_spinner(line: &str) -> bool {
    matches!(line.trim(), "-" | "\\" | "|" | "/")
}

// ---------------------------------------------------------------------------------------------
// Progress parser
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressUpdate {
    pub stage: OperationStage,
    pub fraction: f32,
    pub message: String,
}

const TAIL_LINES: usize = 100;

fn stage_rank(s: OperationStage) -> u8 {
    match s {
        OperationStage::Queued => 0,
        OperationStage::Resolving => 1,
        OperationStage::Downloading => 2,
        OperationStage::Installing => 3,
        OperationStage::Finalizing => 4,
        _ => 5,
    }
}

/// Line-oriented, tolerant progress parser. Fraction and stage never move backwards; unknown
/// lines go to the log tail. The exit code (not this parser) decides success.
#[derive(Debug)]
pub struct ProgressParser {
    stage: OperationStage,
    fraction: f32,
    installer_exit_code: Option<i32>,
    agreements_shown: bool,
    tail: VecDeque<String>,
}

impl Default for ProgressParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressParser {
    pub fn new() -> Self {
        ProgressParser {
            stage: OperationStage::Resolving,
            fraction: 0.0,
            installer_exit_code: None,
            agreements_shown: false,
            tail: VecDeque::new(),
        }
    }

    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    pub fn installer_exit_code(&self) -> Option<i32> {
        self.installer_exit_code
    }

    /// True once winget printed license / agreement text.
    pub fn agreements_shown(&self) -> bool {
        self.agreements_shown
    }

    pub fn tail_text(&self) -> String {
        self.tail.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    /// Feeds one line; returns an update only when the stage or fraction advanced.
    pub fn feed_line(&mut self, raw: &str) -> Option<ProgressUpdate> {
        if is_spinner(raw) {
            return None;
        }
        let line = raw.trim();
        if line.is_empty() {
            return None;
        }
        match self.classify(line) {
            Some((stage, fraction)) => self.advance(stage, fraction, line),
            None => {
                self.push_tail(line);
                None
            }
        }
    }

    fn classify(&mut self, line: &str) -> Option<(OperationStage, f32)> {
        use OperationStage::*;
        if line.starts_with("Found ") && line.contains('[') && line.contains(']') {
            return Some((Resolving, 0.05));
        }
        if line.contains("This application is licensed to you by its owner")
            || line.contains("Microsoft is not responsible for, nor does it grant any licenses")
        {
            self.agreements_shown = true;
            return None;
        }
        if line.starts_with("Downloading ") {
            return Some((Downloading, 0.10));
        }
        if let Some(ratio) = parse_bar(line) {
            return Some((Downloading, 0.10 + 0.50 * ratio));
        }
        if line.starts_with("Successfully verified installer hash") {
            return Some((Installing, 0.65));
        }
        if line.starts_with("Starting package install")
            || line.starts_with("Starting package uninstall")
        {
            return Some((Installing, 0.70));
        }
        if line.starts_with("Successfully installed")
            || line.starts_with("Successfully uninstalled")
        {
            return Some((Finalizing, 0.95));
        }
        if let Some(rest) = line.strip_prefix("Installer failed with exit code:") {
            self.installer_exit_code = rest.trim().trim_end_matches('.').parse().ok();
        }
        None
    }

    fn advance(
        &mut self,
        stage: OperationStage,
        fraction: f32,
        line: &str,
    ) -> Option<ProgressUpdate> {
        let new_stage = if stage_rank(stage) > stage_rank(self.stage) {
            stage
        } else {
            self.stage
        };
        let new_fraction = fraction.max(self.fraction);
        let changed = new_stage != self.stage || new_fraction > self.fraction + 1e-4;
        self.stage = new_stage;
        self.fraction = new_fraction;
        if !changed {
            self.push_tail(line);
            return None;
        }
        Some(ProgressUpdate {
            stage: new_stage,
            fraction: new_fraction,
            message: line.to_string(),
        })
    }

    fn push_tail(&mut self, line: &str) {
        if self.tail.len() == TAIL_LINES {
            self.tail.pop_front();
        }
        self.tail.push_back(line.to_string());
    }
}

/// Returns the completed ratio (0..=1) when `line` is a download bar or a bare percentage.
pub fn parse_bar(line: &str) -> Option<f32> {
    let t = line.trim();
    let filled = t.chars().filter(|c| *c == '█').count();
    let empty = t.chars().filter(|c| *c == '▒').count();
    if filled + empty == 0 {
        return parse_percent(t);
    }
    let rest: String = t.chars().skip_while(|c| matches!(*c, '█' | '▒')).collect();
    let rest = rest.trim();
    if let Some(r) = parse_percent(rest) {
        return Some(r);
    }
    if let Some(r) = parse_sizes(rest) {
        return Some(r);
    }
    Some(filled as f32 / (filled + empty) as f32)
}

fn parse_percent(t: &str) -> Option<f32> {
    let num = t.strip_suffix('%')?.trim();
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let v: f32 = num.parse().ok()?;
    Some((v / 100.0).clamp(0.0, 1.0))
}

fn parse_sizes(rest: &str) -> Option<f32> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 5 || parts[2] != "/" {
        return None;
    }
    let done = to_bytes(parts[0], parts[1])?;
    let total = to_bytes(parts[3], parts[4])?;
    if total <= 0.0 {
        return None;
    }
    Some((done / total).clamp(0.0, 1.0) as f32)
}

fn to_bytes(num: &str, unit: &str) -> Option<f64> {
    let n: f64 = num.parse().ok()?;
    let mult = match unit.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" | "KIB" => 1024.0,
        "MB" | "MIB" => 1024.0 * 1024.0,
        "GB" | "GIB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(n * mult)
}

// ---------------------------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------------------------

pub const EXIT_CTRL_SIGNAL: u32 = 0x8A15_0005;
pub const EXIT_NO_APPLICABLE_INSTALLER: u32 = 0x8A15_0010;
pub const EXIT_HASH_MISMATCH: u32 = 0x8A15_0011;
pub const EXIT_NO_APPLICATIONS_FOUND: u32 = 0x8A15_0014;
pub const EXIT_REQUIRES_ADMIN: u32 = 0x8A15_0019;
pub const EXIT_UPDATE_NOT_APPLICABLE: u32 = 0x8A15_002B;
pub const EXIT_PACKAGE_AGREEMENTS: u32 = 0x8A15_0041;
pub const EXIT_SOURCE_AGREEMENTS: u32 = 0x8A15_0046;
pub const EXIT_APPTERMINATION: u32 = 0x8A15_006A;
pub const EXIT_CANCELLED_BY_USER: u32 = 0x8A15_010C;
/// Win32 ERROR_ELEVATION_REQUIRED passed through from an installer (INFERRED).
pub const EXIT_WIN32_ELEVATION: u32 = 740;

#[derive(Debug)]
pub enum ExitOutcome {
    Success,
    /// winget reported nothing to do (exit 0x8A15002B); treated as Done.
    NoOp,
    Failure(AppError),
}

/// Maps a winget exit code (i32 from `ExitStatus::code`, an HRESULT) to an outcome.
pub fn map_exit(
    code: i32,
    package: &str,
    stderr: &str,
    installer_code: Option<i32>,
) -> ExitOutcome {
    let failure = match code as u32 {
        0 => return ExitOutcome::Success,
        EXIT_UPDATE_NOT_APPLICABLE => return ExitOutcome::NoOp,
        EXIT_NO_APPLICATIONS_FOUND => AppError::PackageNotFound(package.to_string()),
        EXIT_NO_APPLICABLE_INSTALLER => {
            AppError::Unsupported("no installer for this system".to_string())
        }
        EXIT_HASH_MISMATCH => AppError::WingetFailed {
            code,
            stderr: format!(
                "installer hash does not match the manifest (security check; no override offered)\n{stderr}"
            ),
        },
        EXIT_REQUIRES_ADMIN | EXIT_WIN32_ELEVATION => AppError::ElevationRequired,
        EXIT_PACKAGE_AGREEMENTS => AppError::AgreementsRequired {
            source_agreements: false,
            package_agreements: true,
        },
        EXIT_SOURCE_AGREEMENTS => AppError::AgreementsRequired {
            source_agreements: true,
            package_agreements: false,
        },
        EXIT_CTRL_SIGNAL | EXIT_APPTERMINATION | EXIT_CANCELLED_BY_USER => AppError::Cancelled,
        _ => {
            let mut detail = stderr.to_string();
            if let Some(n) = installer_code {
                if !detail.is_empty() {
                    detail.push('\n');
                }
                detail.push_str(&format!("installer exit code: {n}"));
            }
            AppError::WingetFailed {
                code,
                stderr: detail,
            }
        }
    };
    ExitOutcome::Failure(failure)
}

// ---------------------------------------------------------------------------------------------
// Tables (`winget list`, `winget upgrade`)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub name: String,
    pub id: String,
    pub version: String,
    pub available: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedTable {
    pub rows: Vec<TableRow>,
    /// Count from a `N upgrades available.` footer, when present.
    pub footer_count: Option<usize>,
    pub warnings: Vec<String>,
}

/// Parses a fixed-width winget table using column offsets taken from the header line.
/// Never splits rows on whitespace: names contain spaces and trailing cells can be blank.
pub fn parse_table(text: &str) -> ParsedTable {
    let mut out = ParsedTable::default();
    let lines: Vec<&str> = text
        .split(['\r', '\n'])
        .filter(|l| !l.trim().is_empty() && !is_spinner(l))
        .collect();
    let Some(header_idx) = lines.iter().position(|l| l.starts_with("Name")) else {
        return out;
    };
    let header: Vec<char> = lines[header_idx].chars().collect();
    let cols = header_columns(&header);
    let find = |name: &str| cols.iter().position(|(n, _)| n == name);
    let (Some(id_col), Some(ver_col)) = (find("Id"), find("Version")) else {
        out.warnings
            .push("table header has no Id/Version column; no rows returned".into());
        return out;
    };
    let name_col = find("Name").unwrap_or(0);
    let avail_col = find("Available");
    let src_col = find("Source");

    let cell = |chars: &[char], col: usize| -> String {
        let start = cols[col].1.min(chars.len());
        let end = cols
            .get(col + 1)
            .map_or(chars.len(), |(_, s)| (*s).min(chars.len()));
        chars[start..end.max(start)]
            .iter()
            .collect::<String>()
            .trim()
            .to_string()
    };
    let opt = |s: String| if s.is_empty() { None } else { Some(s) };

    for line in &lines[header_idx + 1..] {
        let t = line.trim();
        if t.chars().all(|c| c == '-') {
            continue;
        }
        if let Some(n) = parse_footer(t) {
            out.footer_count = Some(n);
            break;
        }
        let chars: Vec<char> = line.chars().collect();
        let id = cell(&chars, id_col);
        if id.is_empty() {
            out.warnings.push(format!("row without id skipped: {t}"));
            continue;
        }
        out.rows.push(TableRow {
            name: cell(&chars, name_col),
            id,
            version: cell(&chars, ver_col),
            available: avail_col.and_then(|c| opt(cell(&chars, c))),
            source: src_col.and_then(|c| opt(cell(&chars, c))),
        });
    }
    if let Some(n) = out.footer_count {
        if n != out.rows.len() {
            out.warnings.push(format!(
                "footer says {n} but {} rows were parsed",
                out.rows.len()
            ));
        }
    }
    out
}

/// (name, start char offset) for each whitespace-separated header word, in order.
fn header_columns(header: &[char]) -> Vec<(String, usize)> {
    let mut cols = Vec::new();
    let mut i = 0;
    while i < header.len() {
        if header[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < header.len() && !header[i].is_whitespace() {
            i += 1;
        }
        cols.push((header[start..i].iter().collect(), start));
    }
    cols
}

fn parse_footer(t: &str) -> Option<usize> {
    let mut it = t.split_whitespace();
    let n: usize = it.next()?.parse().ok()?;
    let word = it.next()?;
    (word.starts_with("upgrade") || word.starts_with("package")).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing_and_floor() {
        let v = WingetVersion::parse("v1.29.380").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 29, 380));
        assert_eq!(WingetVersion::parse("1.6\r\n").unwrap().patch, 0);
        assert_eq!(
            WingetVersion::parse("v1.7.10861-preview").unwrap().patch,
            10861
        );
        assert!(WingetVersion::parse("garbage").is_none());
        assert!(WingetVersion::parse("v1.5.9999").unwrap() < MIN_VERSION);
        assert!(WingetVersion::parse("v1.6.0").unwrap() >= MIN_VERSION);
        assert!(WingetVersion::parse("v2.0.0").unwrap() >= MIN_VERSION);
    }

    #[test]
    fn splitter_handles_cr_lf_and_partial_chunks() {
        let mut s = LineSplitter::default();
        let mut lines = s.push(b"Found Foo [A.B] Ver");
        assert!(lines.is_empty());
        lines.extend(s.push(b"sion 1\r\n- \r\\ \r|\rDownloading x\n\n"));
        assert_eq!(
            lines,
            ["Found Foo [A.B] Version 1", "-", "\\", "|", "Downloading x"]
        );
        s.push(b"tail");
        assert_eq!(s.finish().as_deref(), Some("tail"));
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn splitter_survives_invalid_utf8_and_split_multibyte() {
        let mut s = LineSplitter::default();
        let bar = "██▒▒ 1.00 MB / 2.00 MB".as_bytes();
        let mut lines = s.push(&bar[..4]); // splits inside a 3-byte glyph
        lines.extend(s.push(&bar[4..]));
        lines.extend(s.push(b"\n"));
        assert_eq!(lines, ["██▒▒ 1.00 MB / 2.00 MB"]);
        let lines = s.push(b"bad \xff\xfe byte\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("bad ") && lines[0].contains('\u{FFFD}'));
    }

    #[test]
    fn spinner_detection() {
        for s in ["-", "\\", "|", "/", "   -   "] {
            assert!(is_spinner(s), "{s:?}");
        }
        for s in ["--", "ab", "", "Found x [y]"] {
            assert!(!is_spinner(s), "{s:?}");
        }
    }

    fn feed_all(lines: &[&str]) -> Vec<ProgressUpdate> {
        let mut p = ProgressParser::new();
        lines.iter().filter_map(|l| p.feed_line(l)).collect()
    }

    #[test]
    fn each_stage_line() {
        use OperationStage::*;
        let ups = feed_all(&[
            "Found Some App [Vendor.SomeApp] Version 1.2.3",
            "This application is licensed to you by its owner.",
            "Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.",
            "Downloading https://example.invalid/installer.exe",
            "Successfully verified installer hash",
            "Starting package install...",
            "Successfully installed",
        ]);
        let got: Vec<_> = ups.iter().map(|u| (u.stage, u.fraction)).collect();
        assert_eq!(
            got,
            [
                (Resolving, 0.05),
                (Downloading, 0.10),
                (Installing, 0.65),
                (Installing, 0.70),
                (Finalizing, 0.95)
            ]
        );
    }

    #[test]
    fn uninstall_lines() {
        let ups = feed_all(&[
            "Found Some App [Vendor.SomeApp]",
            "Starting package uninstall...",
            "Successfully uninstalled",
        ]);
        assert_eq!(ups[1].fraction, 0.70);
        assert_eq!(ups[2].stage, OperationStage::Finalizing);
    }

    #[test]
    fn agreement_text_sets_flag_without_stage_change() {
        let mut p = ProgressParser::new();
        assert!(p
            .feed_line("This application is licensed to you by its owner.")
            .is_none());
        assert!(p.agreements_shown());
    }

    #[test]
    fn bar_with_byte_counts() {
        let r = parse_bar("  ██████████████▒▒▒▒▒▒▒▒▒▒▒▒▒▒  4.00 MB / 8.00 MB").unwrap();
        assert!((r - 0.5).abs() < 1e-6);
        let r = parse_bar("██▒▒  512 KB / 1.00 MB").unwrap();
        assert!((r - 0.5).abs() < 1e-6);
        let r = parse_bar("█  1.00 GB / 4.00 GB").unwrap();
        assert!((r - 0.25).abs() < 1e-6);
    }

    #[test]
    fn bar_without_counts_and_percent() {
        assert!((parse_bar("███▒").unwrap() - 0.75).abs() < 1e-6);
        assert!((parse_bar("  45%").unwrap() - 0.45).abs() < 1e-6);
        assert_eq!(parse_bar("100 %"), Some(1.0));
        assert!(parse_bar("Found 100% of things").is_none());
        assert!(parse_bar("Successfully installed").is_none());
    }

    #[test]
    fn bar_maps_into_download_band_and_is_monotonic() {
        let mut p = ProgressParser::new();
        let a = p.feed_line("██▒▒  1.00 MB / 4.00 MB").unwrap();
        assert!((a.fraction - 0.225).abs() < 1e-5);
        assert_eq!(a.stage, OperationStage::Downloading);
        // a redraw that goes backwards must not lower the fraction or emit
        assert!(p.feed_line("█▒▒▒  0.50 MB / 4.00 MB").is_none());
        let b = p.feed_line("████  4.00 MB / 4.00 MB").unwrap();
        assert!((b.fraction - 0.60).abs() < 1e-5);
        assert!(p.fraction() >= b.fraction);
    }

    #[test]
    fn stage_never_regresses() {
        let mut p = ProgressParser::new();
        p.feed_line("Starting package install...").unwrap();
        // a late "Downloading" line must not pull the stage back
        assert!(p.feed_line("Downloading https://x").is_none());
        assert_eq!(p.fraction(), 0.70);
    }

    #[test]
    fn unknown_lines_go_to_tail_only() {
        let mut p = ProgressParser::new();
        assert!(p.feed_line("Some banner we do not know").is_none());
        assert!(p.feed_line("   ").is_none());
        assert!(p.feed_line("-").is_none());
        assert_eq!(p.tail_text(), "Some banner we do not know");
    }

    #[test]
    fn installer_failure_code_is_captured() {
        let mut p = ProgressParser::new();
        p.feed_line("Installer failed with exit code: 1603");
        assert_eq!(p.installer_exit_code(), Some(1603));
    }

    #[test]
    fn exit_code_table() {
        let id = "Vendor.App";
        let f = |c: u32| map_exit(c as i32, id, "err", None);
        assert!(matches!(f(0), ExitOutcome::Success));
        assert!(matches!(f(0x8A15_002B), ExitOutcome::NoOp));
        assert!(matches!(
            f(0x8A15_0014),
            ExitOutcome::Failure(AppError::PackageNotFound(ref p)) if p == id
        ));
        assert!(matches!(
            f(0x8A15_0010),
            ExitOutcome::Failure(AppError::Unsupported(_))
        ));
        match f(0x8A15_0011) {
            ExitOutcome::Failure(AppError::WingetFailed { code, stderr }) => {
                assert_eq!(code as u32, 0x8A15_0011);
                assert!(stderr.contains("hash"));
            }
            other => panic!("{other:?}"),
        }
        for c in [0x8A15_0019, 740] {
            assert!(matches!(
                f(c),
                ExitOutcome::Failure(AppError::ElevationRequired)
            ));
        }
        assert!(matches!(
            f(0x8A15_0041),
            ExitOutcome::Failure(AppError::AgreementsRequired {
                source_agreements: false,
                package_agreements: true
            })
        ));
        assert!(matches!(
            f(0x8A15_0046),
            ExitOutcome::Failure(AppError::AgreementsRequired {
                source_agreements: true,
                package_agreements: false
            })
        ));
        for c in [0x8A15_0005, 0x8A15_006A, 0x8A15_010C] {
            assert!(matches!(f(c), ExitOutcome::Failure(AppError::Cancelled)));
        }
        match map_exit(0x8A15_0115u32 as i32, id, "boom", Some(1603)) {
            ExitOutcome::Failure(AppError::WingetFailed { stderr, .. }) => {
                assert!(stderr.contains("boom") && stderr.contains("1603"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            f(1),
            ExitOutcome::Failure(AppError::WingetFailed { code: 1, .. })
        ));
    }

    // OBSERVED tables (winget 1.29.380), copied from references/winget-cli.md.
    const UPGRADES: &str = "\
Name                                                               Id                              Version         Available      Source\r
----------------------------------------------------------------------------------------------------------------------------------------\r
7-Zip 26.05 (x64)                                                  7zip.7zip                       26.05           26.06          winget\r
Microsoft Visual C++ 2015-2022 Redistributable (x86) - 14.44.35211 Microsoft.VCRedist.2015+.x86    14.44.35211.0   14.51.36247.0  winget\r
Windows Terminal                                                   Microsoft.WindowsTerminal       1.24.12741.0    1.25.2733.0    winget\r
3 upgrades available.\r
";

    const LIST: &str = "\
Name                       Id                                    Version     Available Source
---------------------------------------------------------------------------------------------
7-Zip 26.05 (x64)          7zip.7zip                             26.05       26.06     winget
Android Studio             Google.AndroidStudio                  2026.2.2.9            winget
Armoury Crate Service      ARP\\Machine\\X64\\Armoury Crate Service 6.7.6.0
";

    #[test]
    fn parses_observed_upgrade_table() {
        let t = parse_table(UPGRADES);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.footer_count, Some(3));
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(t.rows[0].name, "7-Zip 26.05 (x64)");
        assert_eq!(t.rows[0].id, "7zip.7zip");
        assert_eq!(t.rows[0].version, "26.05");
        assert_eq!(t.rows[0].available.as_deref(), Some("26.06"));
        assert_eq!(t.rows[0].source.as_deref(), Some("winget"));
        assert_eq!(
            t.rows[1].name,
            "Microsoft Visual C++ 2015-2022 Redistributable (x86) - 14.44.35211"
        );
        assert_eq!(t.rows[1].id, "Microsoft.VCRedist.2015+.x86");
        assert_eq!(t.rows[2].available.as_deref(), Some("1.25.2733.0"));
    }

    #[test]
    fn parses_observed_list_table_with_blank_cells() {
        let t = parse_table(LIST);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.footer_count, None);
        assert_eq!(t.rows[1].name, "Android Studio");
        assert_eq!(t.rows[1].available, None);
        assert_eq!(t.rows[1].source.as_deref(), Some("winget"));
        let arp = &t.rows[2];
        assert_eq!(arp.name, "Armoury Crate Service");
        assert_eq!(arp.id, "ARP\\Machine\\X64\\Armoury Crate Service");
        assert_eq!(arp.version, "6.7.6.0");
        assert_eq!(arp.available, None);
        assert_eq!(arp.source, None);
    }

    #[test]
    fn table_skips_banner_and_spinner_noise() {
        let noisy = format!("\r-\r\\\r|\rSome source update notice\r\n   - \r\n{LIST}");
        assert_eq!(parse_table(&noisy).rows.len(), 3);
    }

    #[test]
    fn table_footer_mismatch_warns_and_missing_id_is_empty() {
        let bad = UPGRADES.replace("3 upgrades", "5 upgrades");
        let t = parse_table(&bad);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.warnings.len(), 1);
        let t = parse_table("Name Version\n---\nFoo 1.0\n");
        assert!(t.rows.is_empty() && !t.warnings.is_empty());
        assert!(
            parse_table("No installed package found matching input criteria.")
                .rows
                .is_empty()
        );
    }
}
