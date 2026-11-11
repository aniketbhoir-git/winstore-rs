# Stage 6 — Comprehension Test

- **Target:** `docs/SPEC.md` at commit `961b63d`. FeatureRange 1-454, DetailedRange 455-547.
- **Date:** 2026-11-11
- **Reviewer:** `egm-comprehension`, fresh context, no document standard, no other file.
- **Totals:** 0 Blocker, 10 Major, 9 Nit

Note from the reviewer: the harness loaded the repository `CLAUDE.md` into its context. The
reviewer states that it did not use that file.

## What this proposes

**FeatureRange (lines 1-454).** The document specifies an open-source Windows app storefront
built on Tauri 2, a Rust core and a web frontend. It searches public catalogs over HTTPS and
uses the winget CLI as a subprocess to install, update and remove apps. Paid items get only a
browser hand-off to the official Store page. There is no payment, sign-in or licensing logic,
and agreements are never accepted silently. It plans a consent flow for winget agreements:
source consent is stored once per source, and package consent is given once per operation and
never stored. A read-only update check uses `winget upgrade` without `--all`. A localhost axum
server for headless machines comes later.

**DetailedRange (lines 455-547).** The work is cut into twelve ordered work packages (WP0 to
WP11), each with acceptance criteria. WP0 to WP4 are merged. WP5, the MVP install loop, is
built on an open PR but needs §6.6, its gap rows and bug F1 to close. WP6 to WP11 cover the
library and updates, the msstore source, polish, Scoop and Chocolatey, headless mode and
hardening. A test strategy (unit, mocked HTTP, a fake winget, Windows end-to-end) and a
guardrail checklist run before every merge complete the plan.

## How the existing system works, per this document

**FeatureRange.** A single Rust crate already has the error model, config, SQLite persistence,
a community catalog adapter and a winget subprocess executor. The executor parses progress and
cancels by killing the process tree with a Job Object, then `taskkill`. It also has the IPC
commands in §5.4 and a Search and AppDetail UI. The `installs` table is the ledger.
`Installing` and `Uninstalling` are the busy states, and §6.7's tables say how each operation
start and end changes the ledger. On restart, leftover busy rows are fixed. Today
`AppConfig.accept_source_agreements` can set both agreement flags silently. Pin,
`UpdateAvailable` and the exit-code log are not written yet.

**DetailedRange.** WP0 to WP4 are on `main`. WP5 lives on `impl/6--install-loop` (PR #19). The
community catalog has a known bug: it returns unrelated results (F1, #20). `check_updates` and
`update_all` exist as commands but no UI uses them.

## Findings

The reviewer could restate the two ranges. It names findings 2 and 3 as the weakest points for
a new implementer.

### Major

**C1 — lines 13-16 against lines 327 and 467 — contract or plan for WP5.** The status note
says the document is the contract for WP0 to WP5 and that the code is the fact. §6.6 (WP5) is
marked "not built", and line 467 says WP5 needs more work. A reader cannot tell if a WP5
difference means "fix the spec" or "build the missing part".
*Recommendation:* say in lines 13-14 that WP5 is partly built, and that §6.6 and the WP5 gap
rows are plan, not contract.

**C2 — lines 377-379, 411, 425-426 — "stable state" has two meanings.** It is defined as "the
state the ledger had before the operation started", then as "`NotInstalled` or `Failed` when
not". These disagree when the earlier state was `Failed`: a cancelled install with no earlier
version ends in `NotInstalled` (line 411). Line 426 still says a cancel leaves the ledger "in
its stable state". The WP5 acceptance criterion (lines 498-499) depends on the term.
*Recommendation:* define the stable state as a function, for example "a version is recorded:
`Installed` (or `Pinned`); if not: `NotInstalled`". Or keep `Failed` on cancel. Make lines 411
and 426 agree.

**C3 — lines 343-345, 353-354 — the ledger result of `agreements_required`.** Step 2 sends the
ledger back through the "failed" rows of §6.7. For a first install that row gives `Failed` and
an Error log row. The step also says "this result is not a failure in the UI", and step 5 says
a decline "does not change" the ledger. A reader cannot tell which ledger state and log level
this ending gives.
*Recommendation:* add an `agreements_required` row to the end table in §6.7, with its terminal
stage, ledger state, log level and error code.

**C4 — lines 257, 289 — `search_sources` signature.** §5.4 gives the arguments `search_id,
query`. §6.1 says `search_sources(query, filters)`, and nothing defines the filters.
*Recommendation:* make §6.1 match §5.4. Define `filters` or remove it.

**C5 — lines 259-265 — return types with no definition.** `OperationStarted`, `InstallStatus`,
`OperationLogEntry` and `UpdatesSummaryEvent` (with its `outcomes` field) are not defined.
*Recommendation:* add them to §4.2 with their fields.

**C6 — lines 36, 156, 477-478, 543 — which catalog endpoint.** Line 36 names two REST
catalogs. WP2 says "a public JSON catalog endpoint". Guardrail 3 requires "only the HTTPS
endpoints declared here", and no endpoint is declared. Nobody can check the guardrail from
this document.
*Recommendation:* name the endpoint of each adapter, or point to one file that lists them.

**C7 — lines 464, 467 — bug F1.** F1 is a condition to close WP5. The document says only that
the service "returns unrelated results". It does not say what the fix requires.
*Recommendation:* give a one-line definition of done for F1, or say that
`docs/ISSUE-BACKLOG.md` defines it.

**C8 — line 547 against line 447 — the merge checklist cannot pass.** Guardrail 6 requires a
log "with ... exit status". The gap table says the exit code is not stored until WP6. Each
merge before WP6 fails the checklist as written.
*Recommendation:* apply guardrail 6 from WP6, or move that gap into WP5.

**C9 — line 510 — WP8 acceptance.** "Visual parity checklist passes" names a checklist that
does not exist. It does not say what the visuals must match.
*Recommendation:* name the reference and list the items, or link to them.

**C10 — lines 501-506 — WP6 and WP7 depend on behavior with no definition.** "Pins hold" and
"update-all converges" need a pin meaning: a winget pin or a local flag, and what it blocks.
"`is_paid` detection" has no method; line 392 refuses paid items and does not say how a
community package gets `is_paid`.
*Recommendation:* add a short paragraph on pin behavior. State the origin of `is_paid` for each
source.

### Nit

- **C11 — lines 25-28.** Two lead-in sentences that overlap. Delete one.
- **C12 — lines 444-451.** The ids A10, A12, A13 and A24 have no explanation. Add a pointer to
  `docs/reviews/2026-11-11-spec-review.md` in the table header.
- **C13 — line 320.** "Phase 3" is the only mention of phases. Use "WP10".
- **C14 — lines 360-362.** "The two flags" and `AppConfig` appear with no introduction. Name
  the flags, and say that `AppConfig` is the type in `config.rs`.
- **C15 — lines 86-89.** Only `agreements_required` shows its string code. List the code of
  each variant.
- **C16 — lines 206, 212, 311, 382.** `src/pkg`, `is_auto`, "configurable concurrency = 1" and
  the legal edges that live only in `domain/install.rs`. Define each one in the text.
- **C17 — lines 413-414.** A failed uninstall ends in `Failed`; a failed install with an
  earlier version stays `Installed`. Add one sentence of reason.
- **C18 — lines 489-490.** WP4 says "define all Tauri commands/events", and §5.4 lists commands
  that are not built. Say "the commands in §5.4 at that time".
- **C19 — lines 515, 518-521.** Token authentication (WP10), offline MSIX install and winget
  import/export (WP11) have no flow in §6. Add one before those work packages start.

## Notes from the main context

- **C1 repeats Stage 5 finding Z1** from a reader with no other file. The status note needs the
  edit.
- **C2 and C3 are defects in the text that I wrote in this session.** The code behavior is not
  in doubt for C2: a cancelled first install ends in `NotInstalled`. For C3 the code today
  settles `agreements_required` as a failure: `Failed` and an Error row for a first install.
  §6.6 must say if that stays or changes.
- **C4:** §5.4 is the fact; §6.1 is old text.
- **C8:** guardrail 6 predates the gap table. The checklist needs the edit.
