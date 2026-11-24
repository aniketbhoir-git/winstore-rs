//! Helpers shared by integration tests.

use std::path::PathBuf;
use std::sync::OnceLock;

/// The fake is a dev-only cargo example (never part of the app bundle). Build it
/// once and take its path from the build report.
pub fn fake_exe() -> PathBuf {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        let mut cmd = std::process::Command::new(cargo);
        cmd.args([
            "build",
            "--example",
            "fake_winget",
            "--message-format=json-render-diagnostics",
            "--manifest-path",
            manifest,
        ]);
        let test_exe = std::env::current_exe().expect("current_exe");
        // Newer cargo keeps test binaries in a build directory apart from the
        // final artifacts, so the profile is read from the path and the example
        // is located from cargo's own report, not from the test binary's path.
        if test_exe.components().any(|c| c.as_os_str() == "release") {
            cmd.arg("--release");
        }
        cmd.stdout(std::process::Stdio::piped());
        let out = cmd.output().expect("run cargo build --example");
        assert!(out.status.success(), "building fake_winget example failed");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| v["reason"] == "compiler-artifact" && v["target"]["name"] == "fake_winget")
            .filter_map(|v| v["executable"].as_str().map(PathBuf::from))
            .next_back()
            .expect("cargo reported no fake_winget executable")
    })
    .clone()
}
