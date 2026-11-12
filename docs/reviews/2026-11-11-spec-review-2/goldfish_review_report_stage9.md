# Resolution of Stage 9 findings

- **Status:** proposed, waits for maintainer approval
- **Source:** `implementation_readiness_report.md` (P1 to P16), `docs/SPEC.md` at commit
  `ce32d68`
- **Date:** 2026-11-12
- **Counts:** 16 PROPOSE FIX. Three of them hold a design choice (P1, P6, P10); the choice is
  marked.

Checks made in the main context:

- `ui/package.json` has no test tool. No UI test can run today (P10).
- The site of the "cannot stop the tree" case is `winget_cli.rs`, in the cancel loop: it
  writes a Warn row and emits no event (P8).

## Proposals

| Id | Proposed change to `docs/SPEC.md` |
|---|---|
| P1 | **Choice.** WP5 builds the consent dialog in the webview, with the trust-boundary rules. The native dialog stays an open alternative and leaves the spike gate. New decision table in WP5: (a) winget gives the text: the dialog shows it; all consent tests apply. (b) winget gives no text and the maintainer approved the link fallback: the dialog shows the link; all consent tests apply. (c) no text and no approval: the operation stops with the message "agreement text is not available"; no flag is ever passed; the tests for "Accept" do not apply. WP5 can close in case (c). |
| P2 | §4.2: `consent` = `{ "kinds": ["source"] or ["package"] or the two, "version": string or null }`. `start_install` needs a version when `kinds` has `package`. `start_uninstall` accepts the kind `source` only and ignores the version; a `package` kind there returns `invalid_config`. A new spike question checks which agreement flags `winget uninstall` accepts. |
| P3 | §8: the guard sentence becomes *planned*, owner F2 (#21). The one real test today (`tests/winget_smoke.rs`) has only `#[ignore]`. The detection condition is defined by `docs/designs/unattended-testing.md`. |
| P4 | §6.6 "Changes that WP5 owns": a file list. `config.rs`, `ipc/state.rs`, `executor/mod.rs` (`InstallRequest` carries the consent; `remove` gets a request), `executor/winget_cli.rs` (`WingetOptions.accept_agreements`, the six argument builders, `finish()`), `services/install.rs` (`settle()`, the module comment), `ipc/commands.rs`, `ipc/test_support.rs`, `error.rs`, `tauri.conf.json`, `ui/src/lib/{api,types,operations.svelte}.ts`, a new `ui/src/components/ConsentDialog.svelte`, `tests/fixtures/fake_winget.rs`, the skill. The three read-only builders pass no flag after the change. A list of the nine tests to rewrite. |
| P5 | §1.3 and the WP5 list: the JSON of `agreements_required` also carries `source_agreements` and `package_agreements`. |
| P6 | **Choice.** Remove the "agreements declined" log row. After a decline nothing ran, and no command tells the core. The alternative is a new command only for a log row. |
| P7 | §6.6: a new spike question: which exit code does winget give for a `--version` that does not exist? Step 4 then says: the operation ends with the code that the exit code map gives; `package_not_found` is expected and not verified. The message names the version. |
| P8 | §6.7: the condition is "the tree is still alive after the Job Object stop and after `taskkill`" (the Warn row site in `winget_cli.rs`). The core then emits one `store:progress` event with the stage unchanged and the message "cancel failed: the installer continues", with a send that cannot be dropped. The UI disables Cancel for that operation. Test: a stop function that fails. The acceptance text says: when winget completes after a cancel, the real outcome stands (§6.7, "Cancel"). |
| P9 | WP5 acceptance: one check for each change. A test reads `tauri.conf.json` for `script-src 'self'`. `accept_source_agreements` occurs nowhere in `src-tauri/` or the skill. The UI test of P10 covers the read of `error.code`. The pull request shows the skill change. The flag test of a retry runs against the subprocess fake (`tests/install_loop.rs`, `tests/executor_winget.rs`). |
| P10 | **Choice.** The WP5 UI work (the dialog, the disabled Cancel, text-only catalog data) needs UI tests, and no UI test tool exists. Order inside WP5: spike, then the core work, then the UI work. The UI work starts after `docs/designs/unattended-testing.md` is approved and its component-test level exists. The alternative is to merge the UI work with no automated test, against requirement F2. Also: the UI waits for `store:operation-finished` before it shows a failure, so that `agreements_required` opens the dialog; the two handlers in `operations.svelte.ts` change. |
| P11 | §9: each item gets its check. Item 3: the timeout and backoff values are the constants in each adapter; a test asserts that a slow endpoint gives a timeout error. Item 4: the §6.6 test "no operation passes a flag with no consent". Item 6: one `operation_log` row for each operation (§0), with the outcome of the §6.7 table. New items: the CSP test, the text-only test, and "no setting, column or file stores a consent". |
| P12 | WP6 "define before code": the executor decision of §2.2, the operation queue, the one-instance rule, what a pin blocks, the skip conditions of `update_all`. The pin acceptance moves into that list. WP7 "define before code": the winget sources that a read-only call names, the merge priority rule, consent for the `msstore` winget source. |
| P13 | WP4: the commands by name (`app_version`, `search_sources`, `get_app_detail`, `start_install`, `cancel_install`, `check_updates`, `update_all`, `open_store_page`, `start_server`); the install runs against the fake winget. WP7: "resolvable" = a search returns a `PackageSummary` with `is_paid = false`; with `msstore` off the result is `SourceUnavailable`; the branch is named. WP8 and WP11: each work package writes criteria that a test can decide before code; one term, "machine scope"; an import needs one user click for the list, and WP11 must show how that satisfies hard rule 4. |
| P14 | §6.6: name the tests for the two agreement exit codes that exist (`tests/executor_winget.rs`) and `ProgressParser::agreements_shown()` as the start point for spike question 1. |
| P15 | §7: WP6 does not wait for F1. The gap row "recorded version" moves with the F1 fix. The spike runs read-only winget commands on a developer machine; each command that can install runs in CI or a sandbox only. |
| P16 | The wording items of style-9-11, and the removal of the repeated sentence in the WP5 acceptance. |

## The three choices

1. **P1:** a webview dialog in WP5, and WP5 can close when winget gives no text and the app
   stops the operation.
2. **P6:** no log row for a decline.
3. **P10:** the WP5 UI work waits for the approved testing design and its component tests.
