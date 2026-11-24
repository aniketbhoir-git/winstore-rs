//! Guard against real winget in tests: no test file except `winget_smoke.rs` and
//! `winget_catalog_smoke.rs` may call the real-winget constructor. Others use `connect_at` with the subprocess fake.

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

#[test]
fn only_winget_smoke_calls_real_winget_connect() {
    // Built from pieces so this file does not match itself.
    let needle = format!("{}::connect(", "WingetCli");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&root.join("tests"), &mut files);
    // Unit-test modules inside src/.
    let mut src = Vec::new();
    rs_files(&root.join("src"), &mut src);
    files.extend(src.into_iter().filter(|p| {
        p.file_name().is_some_and(|n| n == "tests.rs")
            || p.components().any(|c| c.as_os_str() == "tests")
    }));
    let offenders: Vec<_> = files
        .iter()
        .filter(|p| {
            !p.ends_with("winget_smoke.rs")
                && !p.ends_with("winget_catalog_smoke.rs")
                && !p.ends_with("no_real_winget.rs")
        })
        .filter(|p| std::fs::read_to_string(p).unwrap().contains(&needle))
        .collect();
    assert!(
        offenders.is_empty(),
        "real winget called from: {offenders:?}"
    );
}
