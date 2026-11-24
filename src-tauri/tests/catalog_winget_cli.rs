//! F1 tests: the community catalog through the winget CLI, against the fake winget and the
//! recorded fixtures. No network, no real winget.

use std::sync::Arc;
use std::time::Duration;

mod common;
use common::fake_exe;

use store_lib::catalog::{Catalog, CatalogSource, SearchQuery};
use store_lib::domain::{PackageId, SourceKind};
use store_lib::executor::winget_cli::WingetOptions;
use store_lib::sources::winget_cli::WingetCliSource;

struct Fx {
    dir: tempfile::TempDir,
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/winget/1.29/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

impl Fx {
    fn new(script: &str, extra_env: &[(&str, &str)]) -> (Fx, WingetCliSource) {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("script.txt");
        std::fs::write(&script_path, script).unwrap();
        let mut env = vec![
            (
                "FAKE_WINGET_SCRIPT".to_string(),
                script_path.display().to_string(),
            ),
            (
                "FAKE_WINGET_ARGV".to_string(),
                dir.path().join("argv.txt").display().to_string(),
            ),
        ];
        env.extend(
            extra_env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let source = WingetCliSource::with_path(
            fake_exe(),
            WingetOptions {
                log_pool: None,
                env,
            },
        );
        (Fx { dir }, source)
    }

    fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Invocations without the `--version` call of `probe()`.
    fn calls(&self, verb: &str) -> usize {
        self.lines().iter().filter(|l| l.starts_with(verb)).count()
    }
}

fn catalog(source: WingetCliSource, pool: Option<store_lib::db::DbPool>) -> Catalog {
    Catalog::new(vec![Arc::new(source) as Arc<dyn CatalogSource>], pool, 3600)
}

#[tokio::test]
async fn vlc_search_returns_videolan_vlc_in_the_top_three() {
    let (fx, source) = Fx::new(&format!("file {}\n", fixture("search-vlc.txt")), &[]);
    let rows = catalog(source, None)
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap();
    let want = PackageId::new(SourceKind::WingetCommunity, "VideoLAN.VLC");
    assert!(
        rows.iter().take(3).any(|r| r.id == want),
        "{:?}",
        rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(fx.calls("search"), 1);
    let argv = fx
        .lines()
        .into_iter()
        .find(|l| l.starts_with("search"))
        .unwrap();
    assert!(argv.contains("--query vlc --source winget --count 30 --disable-interactivity"));
}

#[tokio::test]
async fn no_match_gives_an_empty_list() {
    let (_fx, source) = Fx::new(
        &format!("file {}\nexit 0x8A150014\n", fixture("search-none.txt")),
        &[],
    );
    let rows = catalog(source, None)
        .search(&SearchQuery::new("qqqzzzxxx"))
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn same_name_rows_with_different_ids_both_survive_the_merge() {
    let (_fx, source) = Fx::new(&format!("file {}\n", fixture("search-same-name.txt")), &[]);
    let rows = catalog(source, None)
        .search(&SearchQuery::new("sample notes"))
        .await
        .unwrap();
    let ids: Vec<_> = rows.iter().map(|r| r.id.to_string()).collect();
    assert_eq!(rows.len(), 3, "{ids:?}");
    assert_eq!(rows[0].name, rows[1].name);
    assert_ne!(rows[0].id, rows[1].id);
}

#[tokio::test]
async fn agreement_exit_is_agreements_required_and_no_flag_is_passed() {
    let (fx, source) = Fx::new("out terms\nexit 0x8A150046\n", &[]);
    let err = catalog(source, None)
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "agreements_required");
    assert!(!fx.lines().is_empty());
    for l in fx.lines() {
        assert!(!l.contains("--accept-"), "{l}");
    }
}

#[tokio::test]
async fn time_limit_gives_source_unavailable_fast() {
    let (_fx, source) = Fx::new("sleep 5000\n", &[]);
    let source = source.with_limits(Duration::from_millis(300), Duration::from_millis(300));
    let err = catalog(source, None)
        .get_detail(&PackageId::new(SourceKind::WingetCommunity, "A.B"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "source_unavailable");
    assert!(err.to_string().contains("300 ms"), "{err}");
}

#[tokio::test]
async fn old_winget_is_unsupported() {
    let (_fx, source) = Fx::new("", &[("FAKE_WINGET_VERSION", "v1.28.240")]);
    let err = catalog(source, None)
        .search(&SearchQuery::new("vlc"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
}

#[tokio::test]
async fn detail_is_cached_and_runs_one_show() {
    let pool = store_lib::db::init_memory_pool().await.unwrap();
    store_lib::db::run_migrations(&pool).await.unwrap();
    let (fx, source) = Fx::new(&format!("file {}\n", fixture("show-vlc.txt")), &[]);
    let catalog = catalog(source, Some(pool));
    let id = PackageId::new(SourceKind::WingetCommunity, "VideoLAN.VLC");
    let first = catalog.get_detail(&id).await.unwrap();
    let second = catalog.get_detail(&id).await.unwrap();
    assert_eq!(first.summary.publisher, "VideoLAN");
    assert_eq!(first, second);
    // The `--version` call of probe() is not a `show` call and is left out of the count.
    assert_eq!(fx.calls("show"), 1, "{:?}", fx.lines());
    assert!(fx.lines().iter().any(|l| l.starts_with("--version")));
}
