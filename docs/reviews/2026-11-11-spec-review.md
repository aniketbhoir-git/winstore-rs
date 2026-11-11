# Consolidated Review Report — SPEC.md

**Target:** `docs/SPEC.md` (340 lines, as of commit `e890b50`)
**Method:** Elephant-Goldfish Model review (`/egm:egm-review`), 11 reviewer runs in fresh contexts. The five stage reports are kept by the maintainer and are not in this repository.

## Checks made after the review

- **A1 is a defect in the spec only.** `executor/winget_cli.rs` runs `winget upgrade --include-unknown --disable-interactivity` with no `--all`, which lists updates and installs nothing. The spec text at §6.3 still needs the fix.
- **WP5 is committed, not only on disk.** It is on branch `impl/6--install-loop` (PR #19, open). `main` holds WP0 to WP4.
- **Search returns unrelated results (found in a manual test).** A search for "vlc" shows unrelated apps. The cause is upstream: `GET https://api.winget.run/v2/packages?query=vlc&take=5` returns Flock and f.lux, with rows last updated in 2023. This raises A14 and issue #16 to MVP blockers. See `docs/ISSUE-BACKLOG.md`, "Review follow-ups".
**Date:** 2026-11-11
**Mode:** consolidated report, all five stages, no checkpoints
**Split:** FeatureRange 1-264, DetailedRange 265-340 (boundary at `## 7. Work packages`)
**Reviewer context:** `docs/STRATEGY.md`, `docs/ISSUE-BACKLOG.md`, `CLAUDE.md`. SPEC.md has no links to other files.

## Verdict

The spec is not ready to drive more implementation. Three problems come first.

1. **The spec tells the implementer to do something harmful.** §6.3 names `winget upgrade --all` as the update check. That command installs every update. All five stages that read the line reported it.
2. **The spec and the code disagree about where the project is.** §7 reads as if nothing is built. Work packages WP0 to WP4 are on `main`, and the WP5 code (detail page, uninstall, cancel, log tail) is on disk. The implementer chose commands, files and state rules that the spec never defined. `CLAUDE.md` names SPEC.md as the source of truth, and it is not.
3. **The agreement rule has no working path.** Hard rule 4 forbids silent acceptance. §6.2 permits a global setting that accepts everything. The code defaults that setting to off and has no screen to change it, so a real install that needs an agreement fails from the UI.

Counter-argument the author can make: the code has 163 passing tests and the skill file holds the correct winget facts, so a stale spec does no harm. That holds only while one person with full memory does the work. The spec's stated job is a hand-off to a model with no memory. A model that reads §6.3 and §7 as written will run upgrades during a check and will rebuild WP5.

A second point against the reviewers: Stage 7 asked for many additions (alternatives, risks, an IPC table, more walkthroughs, exit-code tables). Stage 8 showed that applying all of them would about double the document and copy facts that the skill and the code already hold. This report follows Stage 8 where the two conflict.

## Stage results

| Stage | Report | Reviewers | Blocker | Major | Nit |
| --- | --- | --- | --- | --- | --- |
| 5 Initial zoom out | `zoom_out_report.md` | zoom-out, inventive-catalyst | 1 | 17 | 5 |
| 6 Comprehension | `comprehension_report.md` | comprehension | 0 | 11 | 8 |
| 7 Critic | `critic_report.md` | design x2, structural x2, style x2 | 5 | 26 | 16 |
| 8 Intermediate zoom out | `intermediate_zoom_out_report.md` | zoom-out | 2 | 9 | 4 |
| 9 Implementation readiness | `implementation_readiness_report.md` | implementation-readiness, codebase-details, style | 6 | 25 | 8 |

All reviewers completed. No reviewer has the status `not run`.

Process notes:
- The Stage 7 synthesizer agent stopped twice with no report. The main context dispatched the Stage 7, 8 and 9 reviewers directly. The synthesizer's own reviewers also reported, so each Stage 7 reviewer ran twice.
- The document was not edited between stages. Stage 8 judged the recommendations, not applied fixes.
- The comprehension reviewer could restate lines 1-264 from the document alone. The document is self-contained as a design. Its gaps are contradictions and undefined terms.

## Action items

Raw finding ids: `S5#n` = Stage 5, `C#n` = Stage 6, `B/M/N` = Stage 7, `Z` = Stage 8, `R/RN` = Stage 9.

### 🔴 Blockers

**A1. Replace the update-check command.** (line 244-245)
`check_updates` must use a read-only command: `winget upgrade` with no package argument, or `winget list --upgrade-available`. State in §5.2 that only `update_all` and `update` may change the system. Check what `run_check_updates` in `ipc/commands.rs` runs today.

**A2. Define one agreement consent model and give it a UI path.** (lines 44-45, 235-237, 338)
Follow Stage 5 #12, not Stage 7 B2: accept package agreements per operation after the user sees them; accept source agreements once per source. A single global flag is the silent pre-acceptance that rule 4 forbids. Then assign the consent dialog, its storage, its command and the decline path to a named work package. Today no screen can set `accept_source_agreements`.

**A3. Make the spec agree with the code that exists.** (lines 108-168, 265-314)
Decide what SPEC.md is: a forward plan, or the contract for what is built. Then:
- Add a status per work package. Record what is on `main` and what is only in the working tree (WP5).
- Correct WP3: winget has no cancel call; the code kills a Job Object and falls back to `taskkill /T /F`.
- Update the §3 tree or mark it "minimum paths".
- Review the WP5 commands the implementer chose (`start_uninstall`, `get_install_status`, `get_operation_log`) and record them.

**A4. Define the WP5 state transitions.** (lines 177, 290-292)
Add a table: (current state, event) -> (new state, terminal stage, log row, error code). Cover cancel in each stage, failure, a second click during an operation, and uninstall during install. Backlog #6 requires "a cancelled install cleans up state" and nothing defines cleanup.

**A5. Hold headless mode behind a decision record.** (lines 53, 98, 154-157, 256-261, 308-309, 335) — blocks WP10 only
A localhost API that installs software with no authentication is open to any web page in a local browser. Do not fix this inside the spec. Answer STRATEGY open question 6 in a decision record: hosting model, authentication in every mode, origin checks, winget on Windows Server, the token against the no-secrets rule. Reduce §6.5 to one gate. Remove axum from the "locked" table until then.

**A6. Settle how the msstore source is reached.** (lines 8-9, 24, 40-41, 133, 297-299) — blocks WP7 only
Keep rule 2 strict. Reach msstore through winget only, not through the undocumented Store REST service. Mark the source off by default and opt-in. On failure return `SourceUnavailable`. Do not apply the Stage 7 M1 rewording of rule 2; it would permit direct REST calls.

### 🟡 Major

**A7. One source of truth for phases.** STRATEGY §1.2 lists update-all as a v1 Must; the spec puts it in WP6 after the MVP. STRATEGY §1.3 calls headless the primary persona and §4 puts it in Phase 3. The backlog lets work after #6 run in parallel; the spec says "in this order". The paid hand-off button is in backlog #6 and in spec WP7. Decide once in STRATEGY and reference it by work package number.

**A8. winget availability and the Windows floor.** State the minimum winget and Windows versions, what the UI shows on `WingetNotFound`, and whether the app guides the user to install winget. Windows Server 2022 does not ship winget.

**A9. One executor in v1.** Remove `PackageManager` from §2 and §6.3; bring it back in WP11 for a local MSIX file. State the selection rule: "the executor is chosen by `SourceKind`; v1 has one". Define the registry in WP9, when a second executor exists.

**A10. Simplify the install state model.** `InstallState` holds the lifecycle only (NotInstalled, Installing, Installed, Uninstalling, Failed). Pin is a flag. Update-available comes from the scan. Remove `auto_update` and `is_auto` until a work package defines auto-update; the scheduled scan only notifies. Decide whether `winget pin` or the ledger is the authority, add pin and unpin commands, and reconcile the ledger with `list_installed()` at startup and after each operation. Say how apps installed outside the app show in the Library.

**A11. One queue for operations that change the system.** Windows Installer runs one MSI install per machine (error 1618). Route install, update and remove through one queue with concurrency 1. The per-package lock is built; keep it and add the queue when WP6 lands. Remove "configurable concurrency = 1".

**A12. Stage-dependent cancel.** Cancel at once in `queued`, `resolving` and `downloading`. In `installing`, record "cancel requested", let the installer finish, then offer uninstall. Define cancel for an elevated install in WP11; today it logs a warning and does not cancel.

**A13. Exit codes decide success.** Parsed progress text is a hint; fall back to an indeterminate bar when the parse fails. "Non-zero means failed" is wrong for reboot-required and already-installed codes. Add an `exit_code` column to `operation_log`, which guardrail 6 already assumes. Keep flags and code tables in the `winget-executor` skill and point to it from §6.

**A14. Catalog backend and package identity.** Write a decision record for the community catalog backend and close follow-up #16 there. Declare each endpoint in one place that §5.1 and guardrail 3 reference; the code uses `https://api.winget.run` and the spec does not say so. Before install, resolve the id against winget with an exact match and stop on a mismatch.

**A15. Keep search simple until a second source exists.** Search uses the network each time; cache detail rows only. Return partial results with a status per source. Defer dedupe, cross-source paging and the per-source offers model to WP9, and say so in §5.3.

**A16. WP5 detail page uses community fields only.** Ratings and screenshots come from the Store catalog in WP7. Limit WP5 acceptance to description, publisher, versions, license and website. State whether the user can choose a version, and what shows when data is absent. Define the log tail's data source.

**A17. `is_paid` and the product id.** State how each source derives `is_paid`. Add a `store_product_id` field. An msstore item with unknown paid status shows the hand-off only.

**A18. Settings in one store.** Theme, source enablement and scan interval each have a `config.rs` field and a database home. Choose one per setting. Specify how the catalog is rebuilt when the user changes sources.

**A19. Reuse what exists.** Move the HTTP policy code out of `winget_repo.rs` to a shared module before WP7. Require new tests to extend `FakeSource`, `FakeExec`, `RecordingSink` and `fake_exe`. Limit WP7 to the adapter and paid detection; the install path, the refusal and the button are built. Move update logic from `ipc/commands.rs` to `services/upgrade.rs`.

**A20. Measurable acceptance and a concrete test plan.** Replace "reliable", "converges", "resolvable", "pins hold" and "visual parity" with assertions. Name the test packages, the CI job and the runner label. Fix the tool choices to what the code uses (`httpmock`, the cargo example). The real-install test in CI conflicts with the owner-approved smoke test; decide which holds.

**A21. File list per remaining work package.** Add a table of paths (create or modify) to WP5 through WP11.

**A22. Short framing sections, by link.** Add a two-line objective, goals and non-goals with links to STRATEGY §1.3 and §5. Add an alternatives entry only for the three open decisions: catalog backend, CLI or COM executor, headless shape. Add only the risks that the spec adds beyond STRATEGY §5. This is narrower than Stage 7 B4 and B5 asked; Stage 8 lowered B4 to Major.

**A23. No-telemetry rule.** Add it to §0. Enforce it with a content security policy that allows HTTPS images only. Decide on an image proxy in WP8. Reword the manual check to a network capture.

**A24. Error variants and scope.** Check `error.rs` against the flows: timeout, rate limit, busy installer. `AgreementsRequired` exists in code and not in §1.3. Fix WP5 to user scope and map an installer that needs admin rights to `ElevationRequired`.

### 🟢 Nits

**A25. Wording and format.** One pass covers these:
- Lock Svelte in §2; move Scoop, Chocolatey and axum to a deferred list.
- Rule 3: allow the text label "Buy in Microsoft Store"; forbid logos and artwork.
- Replace the purpose block (lines 3-11) with one sentence; remove "safe to paste" and the named model.
- Pick one reader; remove "(give these to the coding LLM)" from headings.
- Delete the duplicate lead-in at line 17.
- Rename the `UpdateAvailable` struct; name `AppError` in §1.3; one `ProgressEvent` field list; one enum casing.
- Replace "Phase 3" and "eventually" with WP10.
- Code spans for templated URLs; language tag on the tree; the missing space at line 139; "and" in place of "+".
- Remove unmeasured claims at lines 101-104 ("far safer", "fast").
- Rename "Polish" and "Hardening"; `CatalogSource` lives in `catalog/mod.rs`.

## Conflicts between reviewers, and the call made here

| Topic | Positions | Call |
| --- | --- | --- |
| Agreement consent | Stage 7 B2: one settings flag. Stage 5 #12 and Stage 8 Z1: per operation and per source. | Stage 5 #12. The flag is the defect. |
| Rule 2 and msstore | Stage 7 M1: narrow the rule. Stage 5 #14 and Z2: keep it strict, go through winget. | Keep strict. |
| Headless detail | Stage 7 B3, M10: specify TLS, tokens, identity in the spec. Stage 5 #3 and Z3: cut to a placeholder. | Decision record first. The risk in B3 is real and stays a gate on WP10. |
| Alternatives section | Stage 7 B4: Blocker, every decision. Z6: Major, three open decisions. | Z6. |
| CLI flags in the spec | Stage 7 M4, M5, M17, M23: name the flags. Stage 5 #19 and Z5: behavior in the spec, flags in the skill. | Z5. |
| In-progress states | Stage 7 N15: add `Updating`, `Cancelling`. Stage 5 #7, #16: memory only. | Stage 5. |
| Workspace split | Stage 5 inventive-catalyst: split into three crates now. Stage 5 zoom-out: note only. | Wait for the headless decision record. |

## Traceability matrix

| Action | Stage 5 | Stage 6 | Stage 7 | Stage 8 | Stage 9 |
| --- | --- | --- | --- | --- | --- |
| A1 update check | #1 | #7 | B1 | | |
| A2 agreements | #12 | #1 | B2 | Z1 | R2 |
| A3 spec against code | #20 | | M14 | Z4 | R1, R3, R5, R6, RN8 |
| A4 state transitions | #16, #17 | | M13 | Z7 | R4 |
| A5 headless | #3 | #2, #12 | B3, M10 | Z3, Z12 | R17, R25 |
| A6 msstore | #14 | #5 | M1 | Z2 | R12, R13 |
| A7 phases | #2 | #12, #13 | M11 | Z9 | R7, R8 |
| A8 winget availability | #4 | | M10 | Z9 | |
| A9 one executor | #5, #6 | #4 | M8 | Z11 | R15 |
| A10 state model | #7, #16 | #8 | M3, M18, M26, N15 | Z7, Z12 | R10, R11 |
| A11 one queue | #10 | #15 | M6 | | |
| A12 cancel | #17 | #11 | M5 | Z5 | R5, R18 |
| A13 exit codes | #11, #19 | | M4, M17 | Z5 | R24 |
| A14 catalog and identity | #9, #13 | #3 | M9, M23 | Z13 | R13 |
| A15 search | #15, #18 | #4 | M7, M24 | Z8 | |
| A16 WP5 detail | #8 | #17 | | Z10 | R21 |
| A17 `is_paid` | #14 | #5, #6 | M2 | | R12 |
| A18 settings | | | N14 | | R16 |
| A19 reuse | | | | Z4 | R9, R12, R14, R23 |
| A20 acceptance and tests | | #10, #17 | | | R19, R20, R22, R26, RN1-RN7 |
| A21 file lists | | | | | R6 |
| A22 framing sections | #23 | | B4, B5, M11 | Z4, Z6 | |
| A23 telemetry | | | M25 | Z14 | R26 |
| A24 errors and scope | | #14 | M22 | | R18 |
| A25 wording | #21, #22, #23 | #18 | M15, M16, M19, M20, M21, N1-N16 | Z15 | RN5 |

Stage 6 findings #9 (IPC command set), #16 (undefined fields) and #19 (requirements only in the plan) map to A3, A25 and A13.

## Suggested order

1. A1 — one-line fix; check the code at the same time.
2. A3 — decide what the spec is and record the project status. Later items depend on it.
3. A2 and A4 — both gate the close of WP5 (issue #6).
4. A7 — the phase decision sets the scope of everything after the MVP.
5. A9, A10, A13, A16 — small edits that remove things from the spec.
6. A5 and A6 — decision records, before WP10 and WP7 start.
7. The rest, with A25 as one cleanup pass.
