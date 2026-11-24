//! SPEC §6.6 "Trust boundary": the webview runs only the app's own script files.
//!
//! The core trusts the webview, so a foreign or inline script must not run there. This test
//! keeps `script-src 'self'` in `tauri.conf.json`, for the release policy and the dev policy.

use std::path::Path;

fn policy(key: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let text = std::fs::read_to_string(&path).expect("tauri.conf.json is readable");
    let conf: serde_json::Value = serde_json::from_str(&text).expect("tauri.conf.json is JSON");
    conf["app"]["security"][key]
        .as_str()
        .unwrap_or_else(|| panic!("app.security.{key} must be a string"))
        .to_string()
}

/// The sources of one directive, or `None` when the policy does not name it.
fn directive(policy: &str, name: &str) -> Option<Vec<String>> {
    policy.split(';').find_map(|d| {
        let mut parts = d.split_whitespace();
        (parts.next() == Some(name)).then(|| parts.map(str::to_string).collect())
    })
}

#[test]
fn script_src_is_self_only() {
    for key in ["csp", "devCsp"] {
        let p = policy(key);
        assert_eq!(
            directive(&p, "script-src"),
            Some(vec!["'self'".to_string()]),
            "{key}: script-src must be exactly 'self' (no inline script, no eval, no host)"
        );
        assert_eq!(
            directive(&p, "default-src"),
            Some(vec!["'self'".to_string()]),
            "{key}: default-src must be exactly 'self'"
        );
        // A second script directive could widen the policy again.
        for other in ["script-src-elem", "script-src-attr"] {
            assert_eq!(directive(&p, other), None, "{key}: {other} must be absent");
        }
    }
}

#[test]
fn directive_reader_splits_names_and_sources() {
    let p = "default-src 'self'; script-src 'self' 'unsafe-inline'";
    assert_eq!(
        directive(p, "script-src"),
        Some(vec!["'self'".to_string(), "'unsafe-inline'".to_string()])
    );
    assert_eq!(directive(p, "img-src"), None);
}
