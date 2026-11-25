//! The app never asks for elevation for itself (hardening design §1 rule 1).

const MANIFEST: &str = include_str!("../windows-app-manifest.xml");

#[test]
fn manifest_pins_as_invoker() {
    assert!(MANIFEST.contains(r#"requestedExecutionLevel level="asInvoker""#));
    assert!(!MANIFEST.contains("requireAdministrator"));
}

#[test]
fn manifest_keeps_the_tauri_default_dependency() {
    assert!(MANIFEST.contains("Microsoft.Windows.Common-Controls"));
    assert!(MANIFEST.contains(r#"version="6.0.0.0""#));
    assert!(MANIFEST.contains(r#"publicKeyToken="6595b64144ccf1df""#));
}
