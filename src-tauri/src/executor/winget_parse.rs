//! Pure winget output parsing (no I/O, no process code).
//!
//! Facts and patterns come from `.claude/skills/winget-executor`. Patterns marked INFERRED there
//! (install/uninstall stdout) are unconfirmed until the manual smoke run.

use std::collections::VecDeque;
use std::fmt;

use crate::domain::OperationStage;
use crate::error::{AgreementEntry, AppError};

/// Oldest winget this project supports (SPEC 2; 1.29 prints full columns to a pipe).
pub const MIN_VERSION: WingetVersion = WingetVersion {
    major: 1,
    minor: 29,
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
    /// True while no download bar was seen: the fraction is a stage estimate only.
    pub indeterminate: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgreementBlock {
    None,
    /// `listing` is true after the `Agreements:` line.
    Package {
        listing: bool,
    },
    Source,
}

const AGREEMENT_MAX_ENTRIES: usize = 16;
const AGREEMENT_MAX_TEXT: usize = 4000;
const AGREEMENT_MAX_LABEL: usize = 200;
const URL_MAX_LEN: usize = 2048;

/// A link the app may show and open: `https://`, a host, no space, control or markup
/// character, at most 2048 bytes. Anything else is not a link (SPEC §6.6 trust boundary).
pub fn safe_https_url(s: &str) -> Option<String> {
    let rest = s.strip_prefix("https://")?;
    let host_ok = rest
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    let clean = s.chars().all(|c| {
        c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\' | '`' | '{' | '}' | '|' | '^')
    });
    (host_ok && clean && s.len() <= URL_MAX_LEN).then(|| s.to_string())
}

/// Parses one `Label: text or link` line of an agreement block. A final token that is a safe
/// https link becomes `url`; the rest is `text`. A line with neither is no agreement.
pub fn parse_agreement_entry(line: &str) -> Option<AgreementEntry> {
    let line = line.trim();
    let (label, rest) = match line.split_once(": ") {
        Some((l, r)) => (l.trim(), r.trim()),
        None => (line.strip_suffix(':')?.trim(), ""),
    };
    if label.is_empty() || label.len() > AGREEMENT_MAX_LABEL || label.contains("://") {
        return None;
    }
    let (text, url) = match rest.rsplit_once(char::is_whitespace) {
        Some((before, last)) if safe_https_url(last).is_some() => {
            (before.trim(), safe_https_url(last))
        }
        _ => match safe_https_url(rest) {
            Some(u) => ("", Some(u)),
            None => (rest, None),
        },
    };
    let text: String = text.chars().take(AGREEMENT_MAX_TEXT).collect();
    if text.is_empty() && url.is_none() {
        return None;
    }
    Some(AgreementEntry {
        label: label.to_string(),
        text: (!text.is_empty()).then_some(text),
        url,
    })
}

/// Line-oriented, tolerant progress parser. Fraction and stage never move backwards; unknown
/// lines go to the log tail. The exit code (not this parser) decides success.
#[derive(Debug)]
pub struct ProgressParser {
    stage: OperationStage,
    fraction: f32,
    bar_seen: bool,
    installer_exit_code: Option<i32>,
    agreements_shown: bool,
    block: AgreementBlock,
    agreements: Vec<AgreementEntry>,
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
            bar_seen: false,
            installer_exit_code: None,
            agreements_shown: false,
            block: AgreementBlock::None,
            agreements: Vec::new(),
            tail: VecDeque::new(),
        }
    }

    pub fn fraction(&self) -> f32 {
        self.fraction
    }

    /// True until a download bar (sizes or percent) was seen. winget 1.29.240+ prints none to
    /// a pipe, so the fraction is then only a stage estimate.
    pub fn indeterminate(&self) -> bool {
        !self.bar_seen
    }

    pub fn installer_exit_code(&self) -> Option<i32> {
        self.installer_exit_code
    }

    /// True once winget printed an agreement block (spike E7): the package header
    /// `Agreements for <name> [<id>] Version <v>` or the source header. The notice that winget
    /// prints for every install ("This application is licensed to you by its owner.") does not
    /// count.
    pub fn agreements_shown(&self) -> bool {
        self.agreements_shown
    }

    /// The agreements of the block winget printed (label, text, https link).
    pub fn agreements(&self) -> &[AgreementEntry] {
        &self.agreements
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
        if self.agreement_line(line, raw.starts_with([' ', '\t'])) {
            self.push_tail(line);
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

    /// Reads the agreement block of an install that stops for an agreement. Returns true for a
    /// line that belongs to the block (header, entry or footer).
    fn agreement_line(&mut self, line: &str, indented: bool) -> bool {
        if line.starts_with("Agreements for ") && line.contains('[') {
            self.block = AgreementBlock::Package { listing: false };
            self.agreements_shown = true;
            self.agreements.clear();
            return true;
        }
        if line.starts_with("The `")
            && line.contains("source requires that you view the following agreements")
        {
            self.block = AgreementBlock::Source;
            self.agreements_shown = true;
            self.agreements.clear();
            return true;
        }
        match self.block {
            AgreementBlock::None => false,
            AgreementBlock::Package { listing } => {
                if line == "Agreements:" {
                    self.block = AgreementBlock::Package { listing: true };
                    return true;
                }
                if !listing {
                    return false;
                }
                if !indented {
                    self.block = AgreementBlock::None;
                    return false;
                }
                self.add_entry(line, true);
                true
            }
            AgreementBlock::Source => {
                if line.starts_with("One or more of the source agreements") {
                    self.block = AgreementBlock::None;
                    return true;
                }
                self.add_entry(line, false);
                true
            }
        }
    }

    /// `Label: text or link`; an indented line with no label continues the last text.
    fn add_entry(&mut self, line: &str, may_continue: bool) {
        if let Some(entry) = parse_agreement_entry(line) {
            if self.agreements.len() < AGREEMENT_MAX_ENTRIES {
                self.agreements.push(entry);
            }
        } else if may_continue && !line.ends_with(':') {
            if let Some(last) = self.agreements.last_mut() {
                let text = last.text.get_or_insert_with(String::new);
                if text.len() < AGREEMENT_MAX_TEXT {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(line);
                }
            }
        }
    }

    fn classify(&mut self, line: &str) -> Option<(OperationStage, f32)> {
        use OperationStage::*;
        if line.starts_with("Found ") && line.contains('[') && line.contains(']') {
            return Some((Resolving, 0.05));
        }
        if line.starts_with("Downloading ") {
            return Some((Downloading, 0.10));
        }
        if let Some(ratio) = parse_bar(line) {
            self.bar_seen = true;
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
            indeterminate: !self.bar_seen,
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
pub const EXIT_NO_MANIFEST_FOUND: u32 = 0x8A15_0017;
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
        // The requested version does not exist (spike Q2, SPEC §6.6 step 4).
        EXIT_NO_MANIFEST_FOUND => {
            let line = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            AppError::PackageNotFound(if line.is_empty() {
                format!("{package}: no manifest for the requested version")
            } else {
                line.trim().to_string()
            })
        }
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
            winget_source: None,
            version: None,
            agreement_text: None,
            agreement_url: None,
            agreements: Vec::new(),
            consent_token: None,
        },
        EXIT_SOURCE_AGREEMENTS => AppError::AgreementsRequired {
            source_agreements: true,
            package_agreements: false,
            winget_source: None,
            version: None,
            agreement_text: None,
            agreement_url: None,
            agreements: Vec::new(),
            consent_token: None,
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
// `winget search` columns and `winget show` blocks (catalog, ADR 0001)
// ---------------------------------------------------------------------------------------------

/// One data row of a `winget search` table, cells taken by column position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRow {
    /// Position 0.
    pub name: String,
    /// Position 1. Always contains a `.`.
    pub id: String,
    /// Position 2.
    pub version: String,
    /// Position 3 and later (Match, Source); unused.
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Columns {
    pub rows: Vec<SearchRow>,
    /// True when winget printed the cut-list footer (a line that starts with `<`).
    pub truncated: bool,
    pub warnings: Vec<String>,
}

fn is_separator(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 10 && t.chars().all(|c| c == '-')
}

/// Char offsets where a column starts: offset 0 and every non-space char after a space.
fn column_starts(header: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for i in 1..header.len() {
        if !header[i].is_whitespace() && header[i - 1].is_whitespace() {
            starts.push(i);
        }
    }
    starts
}

/// Parses a `winget search` table by column position. Header words are never read: the header
/// is the line above the first dash line. `None` when there is no dash line (or no header).
pub fn parse_columns(text: &str) -> Option<Columns> {
    let lines: Vec<&str> = text
        .split(['\r', '\n'])
        .filter(|l| !l.trim().is_empty() && !is_spinner(l))
        .collect();
    let sep = lines.iter().position(|l| is_separator(l))?;
    let header: Vec<char> = lines.get(sep.checked_sub(1)?)?.chars().collect();
    let starts = column_starts(&header);
    if starts.len() < 3 {
        return None;
    }
    let mut out = Columns::default();
    for line in &lines[sep + 1..] {
        if line.starts_with('<') {
            out.truncated = true;
            break;
        }
        let chars: Vec<char> = line.chars().collect();
        // Wide characters shift offsets: a column start must follow padding.
        if starts
            .iter()
            .any(|&st| st > 0 && st < chars.len() && !chars[st - 1].is_whitespace())
        {
            out.warnings
                .push(format!("misaligned row skipped (wide characters?): {line}"));
            continue;
        }
        let cells: Vec<String> = starts
            .iter()
            .enumerate()
            .map(|(i, &st)| {
                let start = st.min(chars.len());
                let end = starts
                    .get(i + 1)
                    .map_or(chars.len(), |&e| e.min(chars.len()));
                chars[start..end.max(start)]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .collect();
        let id = &cells[1];
        if id.is_empty() || id.contains(' ') || !id.contains('.') || id.ends_with('\u{2026}') {
            out.warnings
                .push(format!("row with unusable id skipped: {line}"));
            continue;
        }
        if cells[2].is_empty() {
            out.warnings
                .push(format!("row without version skipped: {line}"));
            continue;
        }
        let mut it = cells.into_iter();
        out.rows.push(SearchRow {
            name: it.next().unwrap_or_default(),
            id: it.next().unwrap_or_default(),
            version: it.next().unwrap_or_default(),
            // Real winget leaves the Match cell empty on some rows (no match reason).
            extra: it.filter(|c| !c.is_empty()).collect(),
        });
    }
    Some(out)
}

/// Fields of a `winget show` block that the catalog uses (English keys).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShowBlock {
    pub name: String,
    pub id: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    /// Lines joined with `\n`.
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// `License Url:` of `winget show`, kept only when it is a safe https link.
    pub license_url: Option<String>,
    pub release_notes: Option<String>,
    pub tags: Vec<String>,
    pub installer_type: Option<String>,
    pub installer_url: Option<String>,
    pub installer_sha256: Option<String>,
    pub release_date: Option<String>,
}

impl ShowBlock {
    fn set(&mut self, key: &str, value: &str) {
        let v = (!value.is_empty()).then(|| value.to_string());
        match key {
            "Version" => self.version = v,
            "Publisher" => self.publisher = v,
            "Description" => self.description = v,
            "Homepage" => self.homepage = v,
            "License" => self.license = v,
            "License Url" => self.license_url = v.as_deref().and_then(safe_https_url),
            "Release Notes" => self.release_notes = v,
            "Installer Type" => self.installer_type = v,
            "Installer Url" => self.installer_url = v,
            "Installer SHA256" => self.installer_sha256 = v,
            "Release Date" => self.release_date = v,
            _ => {}
        }
    }

    /// Appends a continuation line to a multi-line text field.
    fn append(&mut self, key: &str, line: &str) {
        let slot = match key {
            "Description" => &mut self.description,
            "Release Notes" => &mut self.release_notes,
            _ => return,
        };
        match slot {
            Some(s) => {
                s.push('\n');
                s.push_str(line);
            }
            None => *slot = Some(line.to_string()),
        }
    }
}

/// Splits `Key: value` at the first colon; the key must look like a key.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once(':')?;
    let k = k.trim();
    (!k.is_empty() && !k.contains("//") && k.len() <= 40).then_some((k, v.trim()))
}

/// Parses `winget show --id <id>` output. `None` when no title line ends with `[<id>]`.
pub fn parse_show(text: &str, id: &str) -> Option<ShowBlock> {
    let suffix = format!("[{id}]").to_lowercase();
    let lines: Vec<&str> = text
        .split(['\r', '\n'])
        .filter(|l| !l.trim().is_empty() && !is_spinner(l))
        .collect();
    let title_at = lines
        .iter()
        .position(|l| l.trim_end().to_lowercase().ends_with(&suffix))?;
    let title = lines[title_at].trim_end();
    // `to_lowercase` can change byte lengths for odd characters; cut by chars from the end.
    let keep = title.chars().count().saturating_sub(suffix.chars().count());
    let before: String = title.chars().take(keep).collect();
    let name = before
        .trim_end()
        .split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest)
        .trim()
        .to_string();
    let mut block = ShowBlock {
        name,
        id: id.to_string(),
        ..ShowBlock::default()
    };
    let mut current: Option<&str> = None;
    for line in &lines[title_at + 1..] {
        let indented = line.starts_with([' ', '\t']);
        let t = line.trim();
        if !indented {
            current = None;
            if let Some((k, v)) = split_key(t) {
                block.set(k, v);
                current = Some(k);
            }
            continue;
        }
        match current {
            Some("Tags") => block.tags.push(t.to_string()),
            Some("Installer") => {
                if let Some((k, v)) = split_key(t) {
                    block.set(k, v);
                }
            }
            Some(k) => block.append(k, t),
            None => {}
        }
    }
    Some(block)
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
    /// True when the header has a `Source` column. winget leaves it out when the command is
    /// limited to one source (`--source winget`).
    pub has_source_column: bool,
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
    out.has_source_column = src_col.is_some();

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
        // winget pads by display width, so wide (CJK) names shift char offsets. A column
        // start must follow padding; if not, the row is misaligned and skipped.
        if cols
            .iter()
            .any(|(_, st)| *st > 0 && *st <= chars.len() && !chars[*st - 1].is_whitespace())
        {
            out.warnings
                .push(format!("misaligned row skipped (wide characters?): {t}"));
            continue;
        }
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
        assert!(WingetVersion::parse("v1.28.240").unwrap() < MIN_VERSION);
        assert!(WingetVersion::parse("v1.29.0").unwrap() >= MIN_VERSION);
        assert_eq!(
            WingetVersion::parse("v1.7.10861-preview").unwrap().patch,
            10861
        );
        assert!(WingetVersion::parse("garbage").is_none());
        assert!(WingetVersion::parse("v1.5.9999").unwrap() < MIN_VERSION);
        assert!(WingetVersion::parse("v1.6.0").unwrap() < MIN_VERSION);
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
    fn piped_1_29_layout_has_no_bar_and_stays_indeterminate() {
        use OperationStage::*;
        let ups = feed_all(&[
            "Found Some App [Vendor.SomeApp] Version 1.2.3",
            "Downloading https://example.invalid/installer.exe",
            "Successfully verified installer hash",
            "Starting package install...",
            "Successfully installed",
        ]);
        let got: Vec<_> = ups.iter().map(|u| u.stage).collect();
        assert_eq!(
            got,
            [Resolving, Downloading, Installing, Installing, Finalizing]
        );
        assert!(ups.iter().all(|u| u.indeterminate), "{ups:#?}");
    }

    #[test]
    fn a_bar_line_ends_indeterminate_for_the_rest_of_the_operation() {
        let mut p = ProgressParser::new();
        assert!(p.indeterminate());
        assert!(
            p.feed_line("Downloading https://x.invalid/a.exe")
                .unwrap()
                .indeterminate
        );
        let bar = p.feed_line("  ████▒▒▒▒  2.00 MB / 8.00 MB").unwrap();
        assert!(!bar.indeterminate);
        assert!(
            !p.feed_line("Successfully verified installer hash")
                .unwrap()
                .indeterminate
        );
        assert!(!p.indeterminate());
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

    fn feed_text(p: &mut ProgressParser, text: &str) {
        for l in text.lines() {
            p.feed_line(l);
        }
    }

    /// Spike E7: the notice that winget prints for every install is not an agreement.
    #[test]
    fn the_install_notice_is_not_an_agreement() {
        let mut p = ProgressParser::new();
        let fixture = include_str!("../../tests/fixtures/winget/1.29/install-piped.txt");
        feed_text(&mut p, fixture);
        assert!(!p.agreements_shown());
        assert!(p.agreements().is_empty());
        p.feed_line("This application is licensed to you by its owner.");
        p.feed_line("Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.");
        assert!(!p.agreements_shown());
    }

    #[test]
    fn package_block_gives_label_and_link() {
        let mut p = ProgressParser::new();
        feed_text(
            &mut p,
            "Found Power BI [Microsoft.PowerBI] Version 2.1\n\
             Agreements for Power BI [Microsoft.PowerBI] Version 2.1\n\
             Version: 2.1\n\
             Publisher: Microsoft\n\
             Agreements:\n\
             \x20 Microsoft Software License Terms: https://powerbi.microsoft.com/desktop-eula/\n\
             Package agreements were not agreed to. Operation cancelled.\n",
        );
        assert!(p.agreements_shown());
        assert_eq!(
            p.agreements(),
            [AgreementEntry {
                label: "Microsoft Software License Terms".into(),
                text: None,
                url: Some("https://powerbi.microsoft.com/desktop-eula/".into()),
            }]
        );
    }

    #[test]
    fn package_block_keeps_text_and_continuation_lines() {
        let mut p = ProgressParser::new();
        feed_text(
            &mut p,
            "Agreements for X [A.B] Version 1\n\
             Agreements:\n\
             \x20 EULA: You may use this app. https://example.org/eula\n\
             \x20   Second line of the text\n\
             \x20 Category: System Components\n",
        );
        let a = p.agreements();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].label, "EULA");
        assert_eq!(
            a[0].text.as_deref(),
            Some("You may use this app.\nSecond line of the text")
        );
        assert_eq!(a[0].url.as_deref(), Some("https://example.org/eula"));
        assert_eq!(a[1].text.as_deref(), Some("System Components"));
        assert_eq!(a[1].url, None);
    }

    #[test]
    fn source_block_gives_label_and_link() {
        let mut p = ProgressParser::new();
        feed_text(
            &mut p,
            "The `msstore` source requires that you view the following agreements before using.\n\
             Terms of Transaction: https://aka.ms/microsoft-store-terms-of-transaction\n\
             The source requires the current machine's 2-letter geographic region to be sent to the backend service to function properly (ex. \"US\").\n\
             One or more of the source agreements were not agreed to. Operation cancelled.\n",
        );
        assert!(p.agreements_shown());
        assert_eq!(p.agreements().len(), 1);
        assert_eq!(p.agreements()[0].label, "Terms of Transaction");
    }

    #[test]
    fn only_safe_https_links_are_links() {
        for bad in [
            "http://example.org/a",
            "javascript:alert(1)",
            "file:///c:/x",
            "https://",
            "https:// example.org",
            "https://e.org/<b>",
            "https://e.org/\"x",
        ] {
            assert_eq!(safe_https_url(bad), None, "{bad}");
        }
        assert!(safe_https_url("https://aka.ms/x?a=1&b=2#f").is_some());
        let e = parse_agreement_entry("Terms: http://example.org/a").unwrap();
        assert_eq!(e.url, None);
        assert_eq!(e.text.as_deref(), Some("http://example.org/a"));
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
        match f(0x8A15_0017) {
            ExitOutcome::Failure(AppError::PackageNotFound(m)) => {
                assert_eq!(m, "err")
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
                package_agreements: true,
                ..
            })
        ));
        assert!(matches!(
            f(0x8A15_0046),
            ExitOutcome::Failure(AppError::AgreementsRequired {
                source_agreements: true,
                package_agreements: false,
                ..
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
    fn wide_character_row_is_skipped_with_warning() {
        let table = format!(
            "Name        Id        Version
----
{}
{}
",
            "日本語      Foo.Bar   1.0", "Plain       Baz.Qux   2.0"
        );
        let t = parse_table(&table);
        assert_eq!(t.rows.len(), 1, "{:?}", t.rows);
        assert_eq!(t.rows[0].id, "Baz.Qux");
        assert_eq!(t.warnings.len(), 1);
        assert!(t.warnings[0].contains("misaligned"));
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

    // -- search columns and show blocks ------------------------------------------------------

    const SEARCH_VLC: &str = include_str!("../../tests/fixtures/winget/1.29/search-vlc.txt");
    const SEARCH_SAME: &str = include_str!("../../tests/fixtures/winget/1.29/search-same-name.txt");
    const SEARCH_NONE: &str = include_str!("../../tests/fixtures/winget/1.29/search-none.txt");
    const SHOW_VLC: &str = include_str!("../../tests/fixtures/winget/1.29/show-vlc.txt");
    const SHOW_MULTILINE: &str =
        include_str!("../../tests/fixtures/winget/1.29/show-multiline.txt");

    #[test]
    fn columns_parse_recorded_vlc_search() {
        let c = parse_columns(SEARCH_VLC).unwrap();
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(!c.truncated);
        assert_eq!(c.rows.len(), 10);
        let first = &c.rows[0];
        assert_eq!(first.name, "VLC media player");
        assert_eq!(first.id, "VideoLAN.VLC");
        assert_eq!(first.version, "3.0.24");
        assert_eq!(first.extra, vec!["Moniker: vlc".to_string()]);
        assert_eq!(c.rows[1].name, "pympress");
        assert_eq!(c.rows[1].extra, vec!["Tag: vlc".to_string()]);
        // Real layout: a name that fills its column, and an empty Match cell.
        assert_eq!(c.rows[8].name, "VLC media player (Nightly)");
        assert_eq!(c.rows[8].id, "VideoLAN.VLC.Nightly");
        assert_eq!(c.rows[8].version, "4.0.0");
        assert!(c.rows[8].extra.is_empty());
    }

    #[test]
    fn columns_without_match_column() {
        let text = "Name     Id        Version\n----------------------\nFoo      A.Foo     1.0\nBar Baz  B.Bar     2.0\n";
        let c = parse_columns(text).unwrap();
        assert_eq!(c.rows.len(), 2);
        assert_eq!(c.rows[1].name, "Bar Baz");
        assert!(c.rows[0].extra.is_empty());
    }

    #[test]
    fn columns_footer_stops_rows_and_sets_truncated() {
        let c = parse_columns(SEARCH_SAME).unwrap();
        assert!(c.truncated);
        assert_eq!(c.rows.len(), 3);
        assert_eq!(c.rows[0].name, c.rows[1].name);
        assert_ne!(c.rows[0].id, c.rows[1].id);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    }

    #[test]
    fn columns_footer_is_not_a_row_even_with_a_dot() {
        let text = "Name   Id        Version\n----------------------\nFoo    A.Foo     1.0\n<more. entries cut>\nBar    B.Bar     2.0\n";
        let c = parse_columns(text).unwrap();
        assert!(c.truncated);
        assert_eq!(c.rows.len(), 1);
    }

    #[test]
    fn columns_skip_id_without_dot_cut_id_and_empty_version() {
        let text = "Name   Id        Version\n----------------------\nFoo    nodot     1.0\nCut    A.Cut\u{2026}    1.0\nNoVer  C.NoVer\nOk     D.Ok      3\n";
        let c = parse_columns(text).unwrap();
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.rows[0].id, "D.Ok");
        assert_eq!(c.warnings.len(), 3);
        // The cut id row is aligned, so the "id ends in an ellipsis" rule rejects it, not the
        // wide-character check.
        assert!(
            c.warnings
                .iter()
                .any(|w| w.starts_with("row with unusable id skipped: Cut")),
            "{:?}",
            c.warnings
        );
        assert!(
            !c.warnings.iter().any(|w| w.contains("misaligned")),
            "{:?}",
            c.warnings
        );
    }

    #[test]
    fn columns_skip_wide_character_row() {
        // winget pads by display width: four CJK chars take eight cells but four chars.
        let text = "Name       Id        Version\n---------------------------\n\u{4e2d}\u{6587}\u{540d}\u{79f0}     A.Wide    1.0\nPlain      B.Plain   2.0\n";
        let c = parse_columns(text).unwrap();
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.rows[0].id, "B.Plain");
        assert_eq!(c.warnings.len(), 1);
    }

    #[test]
    fn columns_none_without_separator_or_header() {
        assert!(parse_columns(SEARCH_NONE).is_none());
        assert!(parse_columns("").is_none());
        assert!(parse_columns("----------------------\nFoo A.Foo 1\n").is_none());
    }

    #[test]
    fn columns_ignore_spinner_noise_and_cr() {
        let text = format!("-\r\\\r|\r{}", SEARCH_VLC.replace('\n', "\r\n"));
        assert_eq!(parse_columns(&text).unwrap().rows.len(), 10);
    }

    #[test]
    fn show_parses_recorded_vlc() {
        let b = parse_show(SHOW_VLC, "videolan.vlc").unwrap();
        assert_eq!(b.name, "VLC media player");
        assert_eq!(b.version.as_deref(), Some("3.0.24"));
        assert_eq!(b.publisher.as_deref(), Some("VideoLAN"));
        assert_eq!(b.homepage.as_deref(), Some("https://www.videolan.org/vlc/"));
        assert_eq!(b.license.as_deref(), Some("GPL-2.0"));
        assert_eq!(
            b.tags,
            [
                "audio-player",
                "audioplayer",
                "cross-platform",
                "dvd",
                "foss",
                "media-player",
                "multimedia",
                "video-player"
            ]
        );
        assert_eq!(b.installer_type.as_deref(), Some("wix"));
        assert!(b.installer_url.as_deref().unwrap().ends_with("win64.msi"));
        assert_eq!(b.installer_sha256.as_deref().map(str::len), Some(64));
        assert_eq!(b.release_date.as_deref(), Some("2026-09-21"));
        assert_eq!(b.release_notes, None);
    }

    #[test]
    fn show_joins_multi_line_description() {
        let b = parse_show(SHOW_VLC, "VideoLAN.VLC").unwrap();
        let d = b.description.unwrap();
        assert!(d.starts_with("VLC is a free and open source"));
        assert!(d.ends_with("\nCompletely Free - no spyware, no ads and no user tracking."));
    }

    #[test]
    fn show_joins_a_value_that_starts_on_the_line_after_its_key() {
        let b = parse_show(SHOW_MULTILINE, "Example.Tool").unwrap();
        assert_eq!(
            b.description.as_deref(),
            Some(
                "Example Tool converts files.
It keeps the original layout."
            )
        );
        assert_eq!(
            b.release_notes.as_deref(),
            Some(
                "Fixed a crash on start.
Faster conversion."
            )
        );
        // Keys after the multi-line value still parse.
        assert_eq!(b.homepage.as_deref(), Some("https://example.org/tool"));
        assert_eq!(b.license.as_deref(), Some("MIT"));
        assert_eq!(b.tags, ["convert"]);
        assert_eq!(b.installer_type.as_deref(), Some("wix"));
    }

    #[test]
    fn show_ignores_unknown_keys_and_missing_title_is_none() {
        let text = "Found Foo [A.Foo]\nVersion: 1\nMystery: x\nPublisher: P\n";
        let b = parse_show(text, "A.Foo").unwrap();
        assert_eq!(b.publisher.as_deref(), Some("P"));
        assert!(parse_show(text, "Other.Id").is_none());
        assert!(parse_show("No package found matching input criteria.", "A.Foo").is_none());
    }
}
