# Resolution of Stage 9 round 2 findings

- **Status:** proposed on 2026-11-12. Waits for maintainer approval.
- **Source:** `implementation_readiness_report_round2.md` (Q1 to Q13), `docs/SPEC.md` at
  commit `a0682b8`
- **Counts:** 13 PROPOSE FIX. Five of them hold a design choice (Q1, Q3, Q4, Q5, Q7); the
  choice is marked.
- **Limit:** the spike changes §6.6 again. Where a fix depends on a spike answer, the fix
  gives a rule for each answer.

## Proposals

| Id | Proposed change to `docs/SPEC.md` |
|---|---|
| Q1 | **Choice.** The `agreements_required` error carries the dialog data. New serialized fields, next to `source_agreements` and `package_agreements`: `winget_source` (string), `version` (the version that the operation gave to winget, or `null`), `agreement_text` (string or `null`), `agreement_url` (string or `null`). The executor fills them before the operation ends. `agreement_text` comes from the place that spike question 1 finds. `agreement_url`: for a package agreement, a new optional `PackageDetail` field `license_url` from the catalog data; for a source agreement, the URL that winget prints, if the spike finds one. The UI rule: text present, show the text (case a); no text and a URL present, show the link (case b, with approval); none of the two, show no dialog and the message of Q4. Add `error.rs`, `domain/package.rs` and `types.ts` to the file table. The alternative is a new command `get_agreement(package_id, version)`; it adds a command and a second round trip. |
| Q2 | The update builder passes no agreement flag in WP5. `PackageExecutor::update(package)` does not change. An update that needs an agreement ends with `agreements_required`, and the UI shows it as an error. WP6 adds "consent for an update" to its "define before code" list. |
| Q3 | **Choice.** Remove `consent.version`. The consent becomes `{ "kinds": [...] }`. One version field stays: `options.version`. `start_install` needs `options.version` when `kinds` has `"package"`; with no version it returns `invalid_config`. The UI sets `options.version` to the `version` field of the error (Q1). With the kind `"source"` only, `options.version` stays optional. New test for the `invalid_config` case. |
| Q4 | **Choice.** The decision table gets a "Core" column. Cases (a) and (b): the core builds the `consent` option. Case (c): WP5 builds no `consent` option and no dialog; the *planned* mark stays in §5.4. In each case, an operation that ends with `agreements_required` and has no text and no approved link ends as in flow step 2 (rest state, Info row), and the UI shows the message "The agreement text is not available. Store cannot install this package." The UI makes this message from `error.code` and the empty fields. Each test in "Tests to add" gets a tag: (a, b, c) or (a, b). The tests for all cases: rest state and Info row; no builder emits a flag with no consent; the CSP test; the text-only UI test; the message above. The tests for (a, b) only: the accepted retry, the version that is gone, the two `invalid_config` tests, "Decline makes no call". WP5 step 1 then says: the spike decides (a) or not (a); the maintainer decides between (b) and (c). |
| Q5 | **Choice.** A rule for each answer of spike question 3. If `winget uninstall` accepts the source agreement flag: as written, and the file table adds `start_uninstall` in `services/install.rs` (it takes the request and passes it to `remove`) and `services/install/tests.rs`. If it accepts no agreement flag: `start_uninstall` gets no `options`, `PackageExecutor::remove` keeps its signature, those table rows do not apply, and an uninstall that ends with `agreements_required` shows as an error. |
| Q6 | §4.2: `ProgressEvent` gets the field `cancel_failed` (boolean, default false). The "cancel failed" event sets it; the UI disables Cancel when it is true and does not match the message text. The send: an awaited send that waits for room, with a 2 s limit; on the limit the executor writes a Warn row and continues. The send must not use the slot kept for the terminal event. The test seam: the function that stops the tree becomes a field of the executor (default: the real `kill_tree`); the test sets one that reports a live tree. |
| Q7 | **Choice.** Question 2 leaves the gate. The flow is correct for each exit code: the code map gives the error and the message names the version. The answer is recorded at the first real-winget run in CI (F2, #21). For question 1: read-only commands first. If they cannot answer, the spike runs the install command by hand in Windows Sandbox or in a manual CI job, never on the developer machine itself. The spike is a one-time investigation, not a required test, so the rule "no required manual step" of §8 does not apply to it. The spike notes record where each command ran. |
| Q8 | Replace the counts with names. Rewrite to consent-driven assertions: `agreement_flags_only_when_accepted`, `pin_agreements_flag_only_when_accepted` (`executor/winget_cli.rs`); `agreements_flags_follow_config`, `agreement_flags_on_update_remove_list_and_pin`, `list_updates_parses_table_and_skips_unmatched_rows` (`tests/executor_winget.rs`); `install_then_remove_one_small_package` (`tests/winget_smoke.rs`). Remove: `default_has_agreements_off` (`config.rs`). Edits that are not a rewrite: `modified_round_trips` (`config.rs`) drops the field and stays; the fixture `Fx::new` in `tests/install_loop.rs`; the helper `Fx::accepting` in `tests/executor_winget.rs`. The `config.rs` table row says "remove the field and one test". |
| Q9 | Guardrail 6: "Each operation has exactly one outcome row in `operation_log`, with a timestamp and the outcome of the §6.7 end table. More Warn and Error rows can exist. Check: the `operation_log` assertions in `tests/install_loop.rs`." |
| Q10 | `pin` is a change, as §5.2 says. Line 629 becomes: the builders with no consent path in WP5 (`list_installed_args`, `list_updates_args`, `pin_args`, `update_args`) pass no flag. The test becomes: "a call with no consent never passes a flag (list, pin, update)". WP6 owns the pin commands and their consent. |
| Q11 | §8 defines two terms: *subprocess fake* (the cargo example `fake_winget`) and `FakeExec` (the in-process executor in `ipc/test_support.rs`). WP5 acceptance: each transition row has a test against one of the two; a test of flags or exit codes uses the subprocess fake. WP3, WP4 and WP6 acceptance say "the subprocess fake". |
| Q12 | The status note and the WP5 status row add: the UI step, and thus the close of WP5 and the merge of PR #19, waits for the approval of `docs/designs/unattended-testing.md`. F2 (#21) owns the component-test level. |
| Q13 | The skill line list adds lines 52, 54 and 161. The status table splits WP7: not started, except the Buy-in-Store button and `open_store_page`, which are built on branch. The CSP test is a new file, `src-tauri/tests/csp_policy.rs`. |

## The five choices

1. **Q1:** the dialog data travels in the `agreements_required` error, not through a new
   command.
2. **Q3:** one version field, `options.version`; `consent.version` goes away.
3. **Q4:** in case (c) the core builds no `consent` option, and the UI shows a fixed message.
4. **Q5:** if `winget uninstall` accepts no agreement flag, uninstall gets no consent at all.
5. **Q7:** spike question 2 does not gate the work; an install command of the spike runs in
   Windows Sandbox or a manual CI job only.

## What these fixes do not close

- Q12 makes a fact visible that was true before: PR #19 cannot merge before the testing
  design is approved and its component-test level exists. That can keep PR #19 open for a
  long time. A split of PR #19 (core work first, UI work in a second pull request) is a
  maintainer decision that this report does not make.
- Q1 adds `license_url` to `PackageDetail`. The backend that ADR 0001 selects must supply it,
  or case (b) has no link for package agreements.
- No audit follows these fixes. Stage 9 used its one re-audit.
