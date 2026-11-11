//! Helpers shared by integration tests.

use std::path::PathBuf;
use std::sync::OnceLock;

/// The fake is a dev-only cargo example (never part of the app bundle). Build it
/// once, then locate it next to this test binary (`target/<profile>/deps/..`).
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
            "--manifest-path",
            manifest,
        ]);
        let test_exe = std::env::current_exe().expect("current_exe");
        let profile_dir = test_exe
            .parent()
            .and_then(|d| d.parent())
            .expect("profile dir");
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("run cargo build --example");
        assert!(status.success(), "building fake_winget example failed");
        let exe = profile_dir
            .join("examples")
            .join(format!("fake_winget{}", std::env::consts::EXE_SUFFIX));
        assert!(exe.exists(), "missing {}", exe.display());
        exe
    })
    .clone()
}
