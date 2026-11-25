//! Guard against real winget in tests. Every `.rs` file under `src/` and `tests/` is scanned,
//! and only test code counts: whole files under `tests/`, `tests.rs` and `test_support.rs`
//! style files, modules declared with `#[cfg(test)] mod x;`, and the bodies of inline
//! `#[cfg(test)] mod x { .. }`. Production code may reach real winget; test code may not.
//! Only `tests/winget_smoke.rs` and `tests/winget_catalog_smoke.rs` are allow-listed (both
//! are ignored and guarded to a hosted runner).
//!
//! Patterns that count as real winget in test code:
//! - `WingetCli::connect(`, and `Self::connect(` inside an `impl WingetCli` file
//! - `WingetCli::discover`
//! - `connect_at(` with a path that is not the fake or a temp-dir path
//! - a renamed import of `WingetCli` (`use ... WingetCli as X`), which hides the others
//! - `AppState::new(` (the default winget factory), and `.core()` on a state that the file
//!   did not build with a fake executor
//!
//! This file builds its patterns from pieces so it does not match itself; its sample strings
//! use `@`, `%` and `$` for the type names and `connect_at` for the same reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let p = entry.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

// -- a small lexer: enough Rust to skip comments, strings and chars --------------------------

/// If a comment, string or char literal starts at `i`, returns the index after it and whether
/// it was a comment.
fn skip_token(b: &[u8], i: usize) -> Option<(usize, bool)> {
    let n = b.len();
    match b[i] {
        b'/' if b.get(i + 1) == Some(&b'/') => {
            let mut j = i;
            while j < n && b[j] != b'\n' {
                j += 1;
            }
            Some((j, true))
        }
        b'/' if b.get(i + 1) == Some(&b'*') => {
            let (mut j, mut depth) = (i + 2, 1);
            while j < n && depth > 0 {
                if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            Some((j, true))
        }
        b'r' if i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') => {
            let mut j = i + 1;
            while b.get(j) == Some(&b'#') {
                j += 1;
            }
            if b.get(j) != Some(&b'"') {
                return None;
            }
            let hashes = j - i - 1;
            j += 1;
            while j < n {
                if b[j] == b'"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&b'#')) {
                    return Some((j + 1 + hashes, false));
                }
                j += 1;
            }
            Some((n, false))
        }
        b'"' => {
            let mut j = i + 1;
            while j < n && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            Some(((j + 1).min(n), false))
        }
        b'\'' => {
            if b.get(i + 1) == Some(&b'\\') {
                let mut j = i + 2;
                while j < n && b[j] != b'\'' {
                    j += 1;
                }
                Some(((j + 1).min(n), false))
            } else if b.get(i + 2) == Some(&b'\'') {
                Some((i + 3, false))
            } else {
                None // a lifetime
            }
        }
        _ => None,
    }
}

/// The text with comments blanked out (same length, newlines kept).
fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match skip_token(b, i) {
            Some((j, true)) => {
                for byte in out.iter_mut().take(j).skip(i) {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                i = j;
            }
            Some((j, false)) => i = j,
            None => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Index of the `}` that closes the `{` at `open`.
fn matching_brace(b: &[u8], open: usize) -> usize {
    let (mut depth, mut i) = (0usize, open);
    while i < b.len() {
        if let Some((j, _)) = skip_token(b, i) {
            i = j;
            continue;
        }
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// `#[cfg(test)]` modules of one comment-free source: the bodies of inline ones, and the
/// names of out-of-line ones (`#[cfg(test)] mod name;`).
fn cfg_test_modules(clean: &str) -> (Vec<String>, Vec<String>) {
    let attr = "#[cfg(test)]";
    let (mut bodies, mut names) = (Vec::new(), Vec::new());
    let mut from = 0;
    while let Some(at) = clean[from..].find(attr) {
        let start = from + at + attr.len();
        from = start;
        let mut rest = clean[start..].trim_start();
        // Skip further attributes and the visibility.
        while rest.starts_with("#[") {
            match rest.find(']') {
                Some(e) => rest = rest[e + 1..].trim_start(),
                None => break,
            }
        }
        if let Some(r) = rest.strip_prefix("pub") {
            rest = r.trim_start();
            if rest.starts_with('(') {
                if let Some(e) = rest.find(')') {
                    rest = rest[e + 1..].trim_start();
                }
            }
        }
        let Some(r) = rest.strip_prefix("mod ") else {
            continue;
        };
        let name_end = r
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(r.len());
        let name = r[..name_end].to_string();
        let after = r[name_end..].trim_start();
        if after.starts_with(';') {
            names.push(name);
        } else if after.starts_with('{') {
            let open = clean.len() - after.len();
            let close = matching_brace(clean.as_bytes(), open);
            bodies.push(clean[open..close.min(clean.len())].to_string());
        }
    }
    (bodies, names)
}

// -- the patterns -----------------------------------------------------------------------------

/// Joins pieces: patterns are built at run time so this file never contains them.
fn p(parts: &[&str]) -> String {
    parts.concat()
}

/// The argument text of the call that starts right after `open` (a `(`), up to its `)`.
fn call_args(text: &str, open: usize) -> &str {
    let b = text.as_bytes();
    let (mut depth, mut i) = (0usize, open);
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    &text[open + 1..]
}

/// `text` with the inside of string and char literals blanked, so a path such as
/// `"fake_exe"` in a literal does not pass for the fake.
fn blank_literals(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match skip_token(b, i) {
            Some((j, false)) => {
                for byte in out.iter_mut().take(j.saturating_sub(1)).skip(i + 1) {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                i = j;
            }
            Some((j, true)) => i = j,
            None => i += 1,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The innermost `fn` of `code` that contains byte `pos`; all of `code` when there is none.
fn enclosing_fn(code: &str, pos: usize) -> &str {
    let b = code.as_bytes();
    let mut best: Option<(usize, usize)> = None;
    let mut from = 0;
    while let Some(at) = code[from..].find("fn ") {
        let start = from + at;
        from = start + 3;
        if start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_') {
            continue;
        }
        let Some(open) = code[start..].find('{').map(|o| start + o) else {
            continue;
        };
        let close = matching_brace(b, open);
        if start <= pos && pos <= close && best.is_none_or(|(s, _)| start > s) {
            best = Some((start, close));
        }
    }
    best.map_or(code, |(s, e)| &code[s..(e + 1).min(code.len())])
}

/// First top-level argument of the call whose `(` is at `open`, literals blanked.
fn first_arg(code: &str, open: usize) -> String {
    let args = blank_literals(call_args(code, open));
    let (mut depth, mut end) = (0i32, args.len());
    for (i, c) in args.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                end = i;
                break;
            }
            _ => {}
        }
    }
    args[..end].to_string()
}

/// A path that never reaches the real winget: it comes from the fake, or from a temp dir
/// that the same function created.
fn path_is_safe(arg: &str, fn_body: &str) -> bool {
    if arg.contains("fake_exe") {
        return true;
    }
    let mut rest = arg;
    while let Some(at) = rest.find(".path()") {
        let mut ident: Vec<char> = rest[..at]
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        ident.reverse();
        let ident: String = ident.into_iter().collect();
        let made = fn_body
            .match_indices(&format!("let {ident} ="))
            .any(|(i, _)| {
                let line = fn_body[i..].lines().next().unwrap_or("");
                line.contains("tempdir(") || line.contains("TempDir::new(")
            });
        if !ident.is_empty() && made {
            return true;
        }
        rest = &rest[at + 7..];
    }
    false
}

/// True when some call of `name(` has a first argument that is not a safe path.
fn has_unsafe_path_call(code: &str, name: &str) -> bool {
    let b = code.as_bytes();
    let mut from = 0;
    while let Some(at) = code[from..].find(name) {
        let start = from + at;
        let open = start + name.len() - 1;
        from = start + name.len();
        // `name` must not be the tail of a longer identifier.
        if start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_') {
            continue;
        }
        if !path_is_safe(&first_arg(code, open), enclosing_fn(code, start)) {
            return true;
        }
    }
    false
}

/// Names of the patterns that `test_code` (comment-free) breaks. `file` is the whole file
/// the code is from, for the one rule that depends on it.
fn violations(test_code: &str, file: &str) -> Vec<&'static str> {
    let cli = p(&["Winget", "Cli"]);
    let mut found = Vec::new();
    if test_code.contains(&p(&[&cli, "::connect("])) {
        found.push("real connect");
    }
    if file.contains(&p(&["impl ", &cli])) && test_code.contains(&p(&["Self", "::connect("])) {
        found.push("Self connect in impl");
    }
    if test_code.contains(&p(&[&cli, "::discover"])) {
        found.push("discover");
    }
    if has_unsafe_path_call(test_code, &p(&["connect", "_at("])) {
        found.push("connect_at with a real path");
    }
    if has_unsafe_path_call(test_code, &p(&[&cli, "::new("])) {
        found.push("new with a real path");
    }
    if test_code.contains(&p(&[&cli, "Source::new("])) {
        found.push("source defaults to discover");
    }
    if test_code.contains(&p(&[&cli, " as "])) {
        found.push("renamed import");
    }
    if test_code.contains(&p(&["AppState", "::new("])) {
        found.push("default state");
    }
    let mut from = 0;
    while let Some(at) = test_code[from..].find(&p(&[".co", "re()"])) {
        let pos = from + at;
        from = pos + 1;
        let body = enclosing_fn(test_code, pos);
        if ![
            "with_core(",
            "state_with(",
            "state_with_toasts(",
            "Core::new(",
        ]
        .iter()
        .any(|b| body.contains(b))
        {
            found.push("core without a fake executor");
            break;
        }
    }
    found
}

/// `rel` is relative to the crate root. Any `tests` component, `test_support.rs` and the
/// modules declared `#[cfg(test)] mod x;` are test code.
fn is_test_file(rel: &Path, test_mods: &BTreeSet<String>) -> bool {
    let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let dir = rel
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|d| d.to_str())
        .unwrap_or("");
    rel.components().any(|c| c.as_os_str() == "tests")
        || stem == "tests"
        || stem == "test_support"
        || test_mods.contains(stem)
        || (stem == "mod" && test_mods.contains(dir))
}

fn scan_tree(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rs_files(&root.join("src"), &mut files);
    rs_files(&root.join("tests"), &mut files);
    let sources: Vec<(PathBuf, String)> = files
        .into_iter()
        .map(|f| {
            let text = strip_comments(&std::fs::read_to_string(&f).unwrap());
            (f, text)
        })
        .collect();
    let test_mods: BTreeSet<String> = sources
        .iter()
        .flat_map(|(_, t)| cfg_test_modules(t).1)
        .collect();
    let mut offenders = Vec::new();
    for (path, text) in &sources {
        if ["winget_smoke.rs", "winget_catalog_smoke.rs"]
            .iter()
            .any(|f| path.file_name().is_some_and(|n| n == *f))
        {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(path);
        let code = if is_test_file(rel, &test_mods) {
            vec![text.clone()]
        } else {
            cfg_test_modules(text).0
        };
        for part in code {
            for v in violations(&part, text) {
                offenders.push(format!("{}: {v}", path.display()));
            }
        }
    }
    offenders
}

#[test]
fn no_test_code_calls_real_winget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let offenders = scan_tree(root);
    assert!(
        offenders.is_empty(),
        "real winget reachable from test code: {offenders:#?}"
    );
}

// -- self-tests: the scanner flags each pattern ----------------------------------------------

fn sample(text: &str) -> String {
    text.replace('@', &p(&["Winget", "Cli"]))
        .replace('%', &p(&["App", "State"]))
        .replace('$', &p(&["connect", "_at"]))
        .replace('~', "core")
        .replace('^', &p(&["Winget", "CliSource"]))
}

fn flagged(code: &str) -> Vec<&'static str> {
    let code = strip_comments(&sample(code));
    violations(&code, &code)
}

#[test]
fn scanner_flags_each_real_winget_pattern() {
    assert_eq!(
        flagged("let c = @::connect(o).await;"),
        ["real connect"],
        "plain connect"
    );
    assert!(flagged("impl @ { fn t() { Self::connect(o); } }").contains(&"Self connect in impl"));
    assert_eq!(flagged("let p = @::discover().await;"), ["discover"]);
    assert_eq!(
        flagged("let c = @::$(real_path, o);"),
        ["connect_at with a real path"]
    );
    assert_eq!(
        flagged("use store_lib::executor::@ as Cli2;"),
        ["renamed import"]
    );
    assert_eq!(
        flagged("let c = @::new(real_path, o);"),
        ["new with a real path"]
    );
    assert_eq!(
        flagged("let c = ^::new();"),
        ["source defaults to discover"]
    );
    // A literal that says "fake_exe" is not the fake; the temp dir of another function is
    // not this function's temp dir; `core()` needs a fake state in its own function.
    assert_eq!(
        flagged("fn t() { @::$(\"fake_exe\", o) }"),
        ["connect_at with a real path"]
    );
    assert_eq!(
        flagged("fn a() { let dir = tempfile::tempdir(); } fn b() { @::$(dir.path(), o) }"),
        ["connect_at with a real path"]
    );
    assert_eq!(
        flagged("fn a() { let s = state_with(f); } fn b() { let c = s.~(); }"),
        ["core without a fake executor"]
    );
    assert_eq!(
        flagged("let s = %::new(); let c = s.~().await;"),
        ["default state", "core without a fake executor"]
    );
    assert_eq!(
        flagged("let c = state.~().await;"),
        ["core without a fake executor"]
    );
}

#[test]
fn scanner_accepts_the_fake_and_ignores_comments() {
    assert!(flagged("@::$(fake_exe(), o).await").is_empty());
    let tmp = "fn t() { let dir = tempfile::tempdir().unwrap(); ";
    assert!(flagged(&format!("{tmp}@::$(dir.path().join(\"absent.exe\"), o) }}")).is_empty());
    assert!(flagged(&format!("{tmp}@::new(dir.path().join(\"x\"), o) }}")).is_empty());
    assert!(flagged("let c = @::new(fake_exe(), o);").is_empty());
    assert!(flagged("let s = state_with(f); s.~().await").is_empty());
    assert!(flagged("// @::connect(o) in a comment\nlet x = 1;").is_empty());
    assert!(flagged("/* @::discover() */ let x = 1;").is_empty());
}

#[test]
fn scanner_reads_only_test_code_of_a_source_file() {
    let src = strip_comments(&sample(
        "fn prod() { @::connect(o); }\n\
         #[cfg(test)]\nmod tests {\n    fn t() { let s = \"}\"; @::connect(o); }\n}\n\
         fn more() { @::discover(); }\n\
         #[cfg(test)]\nmod split;\n",
    ));
    let (bodies, names) = cfg_test_modules(&src);
    assert_eq!(names, ["split"]);
    assert_eq!(bodies.len(), 1);
    // The inline test body is flagged; the production calls around it are not in it.
    assert_eq!(violations(&bodies[0], &src), ["real connect"]);
    assert!(!bodies[0].contains("discover"));
    let mods: BTreeSet<String> = names.into_iter().collect();
    assert!(is_test_file(Path::new("src/x/split.rs"), &mods));
    assert!(is_test_file(Path::new("src/x/split/mod.rs"), &mods));
    assert!(is_test_file(Path::new("src/x/tests.rs"), &mods));
    assert!(is_test_file(Path::new("tests/common/mod.rs"), &mods));
    assert!(is_test_file(
        Path::new("tests/common/helpers/util.rs"),
        &mods
    ));
    assert!(!is_test_file(Path::new("src/x/other.rs"), &mods));
}
