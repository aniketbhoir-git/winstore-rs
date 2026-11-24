# Shared IPC fixtures

One JSON file per payload. Rust checks each file in `src/ipc/tests.rs` (`fixture_*` tests);
the UI tests (L2, `ui/src/**/*.test.ts`) import the same files. A field change in Rust fails
the Rust test until the file is updated (`STORE_WRITE_FIXTURES=1 cargo test fixture_`).

| File | Used by |
|---|---|
| `error_agreements_required.json` | `ui/src/components/InstallButton.test.ts` |
| `event_progress.json` | `ui/src/components/InstallButton.test.ts` |
| `event_operation_finished.json` | `ui/src/components/InstallButton.test.ts` |
| `event_search_results.json` | For WP5 step 3 (search page tests). Checked by Rust only until then. |
