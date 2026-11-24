//! Read-only smoke test of the catalog calls against the REAL winget (plan step 6 of
//! `docs/designs/catalog-winget-cli.md`). It installs nothing and never passes an agreement
//! flag. It runs only on a GitHub-hosted runner: the same guard as `winget_smoke.rs` refuses
//! every other machine before any winget process starts. `#[ignore]` keeps it out of
//! `cargo test` and `scripts/test.ps1`.
//!
//! It proves: (1) the `winget` source asks for no agreement on a fresh machine, (2) the
//! parser reads real output, (3) the first and second `show` times. It also saves the raw
//! output of the read-only calls so the fixtures can be re-recorded.
//!
//! Env: `STORE_CATALOG_ARTIFACTS` (folder for the raw output; default `target/catalog-smoke`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use store_lib::error::AppError;
use store_lib::executor::winget_cli::{WingetCli, WingetOptions};

/// Message of a refused run. It names no variable.
const REFUSAL: &str = "This test runs real winget. It runs only on a GitHub-hosted runner.";

/// Pure guard: `get` looks up an environment value. Allowed only on a GitHub-hosted runner.
fn guard(get: impl Fn(&str) -> Option<String>) -> Result<(), &'static str> {
    let hosted = get("GITHUB_ACTIONS").as_deref() == Some("true")
        && get("RUNNER_ENVIRONMENT").as_deref() == Some("github-hosted");
    if hosted {
        Ok(())
    } else {
        Err(REFUSAL)
    }
}

fn artifact_dir() -> PathBuf {
    let dir = std::env::var("STORE_CATALOG_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/catalog-smoke"));
    std::fs::create_dir_all(&dir).expect("create artifact folder");
    dir
}

/// Runs the read-only winget command with stdout and stderr piped (a pipe, not a console)
/// and saves the raw output as text. Returns the stdout text.
fn record(dir: &Path, winget: &Path, name: &str, args: &[&str]) -> String {
    let out = Command::new(winget)
        .args(args)
        .output()
        .expect("start winget");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let exit = out
        .status
        .code()
        .map_or("none".to_string(), |c| c.to_string());
    std::fs::write(dir.join(format!("{name}.stdout.txt")), &stdout).unwrap();
    std::fs::write(
        dir.join(format!("{name}.meta.txt")),
        format!("args: {args:?}\nexit: {exit}\nstderr:\n{stderr}\n"),
    )
    .unwrap();
    println!("recorded {name}: exit {exit}, {} bytes", stdout.len());
    stdout
}

/// Notes whether columns look cut when stdout is a pipe: winget ends a cut cell with U+2026.
fn columns_note(search_out: &str) -> String {
    let lines: Vec<&str> = search_out.lines().collect();
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let ellipsis = lines.iter().filter(|l| l.contains('\u{2026}')).count();
    let header = lines
        .iter()
        .find(|l| l.trim_start().starts_with("Name"))
        .copied()
        .unwrap_or("(no header line)");
    format!(
        "stdout is a pipe.\nwidest line: {widest} chars\nlines with an ellipsis (cut cell): {ellipsis}\nheader: {header}\ncolumns cut: {}\n",
        if ellipsis > 0 { "YES" } else { "no" }
    )
}

/// A read-only call must not ask for an agreement. If it does, plan step 6 says stop.
fn fail_on_agreement<T>(r: Result<T, AppError>) -> T {
    match r {
        Err(AppError::AgreementsRequired { .. }) => panic!(
            "agreements_required on a read-only call: the winget source asks for an agreement \
             on a fresh machine. Stop and report to the owner (SPEC 6.6)."
        ),
        other => other.expect("read-only winget call failed"),
    }
}

#[tokio::test]
#[ignore = "runs real winget (read-only); GitHub-hosted runner only"]
async fn real_winget_catalog_search_and_show() {
    // The guard runs first: a refused run starts no winget process.
    if let Err(msg) = guard(|k| std::env::var(k).ok()) {
        panic!("{msg}");
    }
    let dir = artifact_dir();

    let cli = WingetCli::connect(WingetOptions::default())
        .await
        .expect("winget not found or too old");
    let version = cli.probe().await.expect("winget --version");
    println!("winget at {} ({version})", cli.path().display());

    // Raw output first, so it is saved even if an assertion below fails.
    let ver = Command::new(cli.path()).arg("--version").output().unwrap();
    std::fs::write(dir.join("winget-version.txt"), &ver.stdout).unwrap();
    let common = ["--source", "winget", "--disable-interactivity"];
    let mut vlc = vec!["search", "--query", "vlc"];
    vlc.extend(common);
    let vlc_out = record(&dir, cli.path(), "search-vlc", &vlc);
    let mut nonsense = vec!["search", "--query", "zzzqqqxxnotapackage"];
    nonsense.extend(common);
    record(&dir, cli.path(), "search-nonsense", &nonsense);
    let mut show = vec!["show", "--id", "VideoLAN.VLC", "--exact"];
    show.extend(common);
    record(&dir, cli.path(), "show-vlc", &show);
    std::fs::write(dir.join("columns-in-pipe.txt"), columns_note(&vlc_out)).unwrap();

    // The app's own read-only calls.
    let rows = fail_on_agreement(cli.search_catalog("vlc", 10).await);
    println!("search vlc: {} rows", rows.len());
    assert!(
        rows.iter().take(3).any(|r| r.id == "VideoLAN.VLC"),
        "VideoLAN.VLC not in the top 3: {:?}",
        rows.iter().take(5).map(|r| &r.id).collect::<Vec<_>>()
    );

    let none = fail_on_agreement(cli.search_catalog("zzzqqqxxnotapackage", 10).await);
    assert!(none.is_empty(), "nonsense query returned {none:?}");

    let t1 = Instant::now();
    let first = fail_on_agreement(cli.show_package("VideoLAN.VLC").await);
    let first_t = t1.elapsed();
    let t2 = Instant::now();
    let second = fail_on_agreement(cli.show_package("VideoLAN.VLC").await);
    let second_t = t2.elapsed();
    println!("show VideoLAN.VLC first:  {first_t:?}");
    println!("show VideoLAN.VLC second: {second_t:?}");
    std::fs::write(
        dir.join("show-timing.txt"),
        format!("first: {first_t:?}\nsecond: {second_t:?}\n"),
    )
    .unwrap();
    assert_eq!(first.id, "VideoLAN.VLC");
    assert_eq!(second.id, "VideoLAN.VLC");
}

#[cfg(test)]
mod guard_tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn allows_only_github_hosted_runner() {
        let ok = [
            ("GITHUB_ACTIONS", "true"),
            ("RUNNER_ENVIRONMENT", "github-hosted"),
        ];
        assert_eq!(guard(env(&ok)), Ok(()));
        let cases: [&[(&str, &str)]; 4] = [
            &[],
            &[("GITHUB_ACTIONS", "true")],
            &[
                ("GITHUB_ACTIONS", "true"),
                ("RUNNER_ENVIRONMENT", "self-hosted"),
            ],
            &[("RUNNER_ENVIRONMENT", "github-hosted")],
        ];
        for c in cases {
            assert_eq!(guard(env(c)), Err(REFUSAL));
        }
        assert!(!REFUSAL.contains("GITHUB_ACTIONS") && !REFUSAL.contains("RUNNER_ENVIRONMENT"));
    }

    #[test]
    fn columns_note_flags_a_cut_cell() {
        let cut = "Name  Id\n----\nVLC media pl\u{2026}  VideoLAN.VLC\n";
        assert!(columns_note(cut).contains("columns cut: YES"));
        assert!(columns_note("Name  Id\n----\nVLC  VideoLAN.VLC\n").contains("columns cut: no"));
    }
}
