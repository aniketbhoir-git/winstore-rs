# Stage 5 — Initial Zoom Out Review

- **Target:** `docs/designs/unattended-testing.md` (158 lines), commit `05d3f9d`
- **Date:** 2026-11-12
- **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst`, each in a fresh context
- **Totals:** 0 Blocker, 10 Major, 7 Nit

## Verdict

Revise. The core meets F2: a required L1 and L2 check, a guard in test code, fork runs with no
secret. Three parts are larger than F2 needs, the guard can be bypassed, and two pieces of
scope that the SPEC gives to this design are missing.

The weakest assumption: that environment variables keep real installs in CI or in a sandbox.

## Zoom out findings

| Id | Sev | Lines | Finding | Recommendation |
|---|---|---|---|---|
| Z-1 | Major | 65-67 | `STORE_E2E_ISOLATED=1` is a bypass that the design documents. A person or a model that sees a refused test can set it. `GITHUB_ACTIONS=true` is as easy to set. | Remove the variable. Require `RUNNER_ENVIRONMENT=github-hosted` with `GITHUB_ACTIONS`. The refusal message names no condition that a person can set. |
| Z-2 | Major | 51, 77-79, 105, 142 | L3 (WebDriver) is not required, depends on an unchecked Edge driver match, and F2 does not need it. | Move L3 to the non-goals as a follow-up issue. |
| Z-3 | Major | 61-64, 107, 131-132, 143 | The Windows Sandbox script is an optional local path that F2 does not ask for. It brings two unchecked assumptions and a Pro-only dependency. | Drop the script and `-Sandbox`. Keep the `WDAGUtilityAccount` branch of the guard. |
| Z-4 | Major | 22, 139-140 | SPEC §7 WP5 step 3 waits for the "component-test level". Work item 3 does not name the consent dialog, the UI tests of SPEC §6.6 or the mocked events. | Give L2 an exit criterion. |
| Z-5 | Major | whole file | SPEC §8 gives four checks to this design: elevation prompt, cancel during a download, offline behavior, no network call outside hard rule 5. The design covers elevation only. | A table that gives each one a level or a deferred owner. |
| Z-6 | Major | 52, 136-137 | The design does not say if the L4 run in CI accepts agreements. The smoke test accepts them only with `STORE_SMOKE_ACCEPT`. This touches hard rule 4. | State the L4 agreement policy. L4 records the answer to SPEC §6.6 spike question 2. |
| Z-7 | Nit | 93-97 | Three triggers for one job that is not required. | Start with the manual start and the label. |
| Z-8 | Nit | 74-76, 140 | "The handlers produce those files" has no rule for fields that change in each run. | Limit the fixtures and say how such fields are handled. |
| Z-9 | Nit | 50 | The DOM environment for Vitest has no name. CLAUDE.md permits no dependency outside SPEC §2. | Name it; add a work item that lists the dev dependencies in SPEC §2. |
| Z-10 | Nit | 35, 37, 136 | The test paths omit `src-tauri/`. | Paths from the repository root. |
| Z-11 | Nit | 103-107 | "One parameter", and the list shows two. | Keep `-Level` only. |

## Inventive findings (advisory)

| Id | Sev | Lines | Contradiction | Transform |
|---|---|---|---|---|
| I-1 | Major | 74-76, 126-127 | L2 is the required UI gate and cannot see a gap between mock and Rust; the fixtures omit event sequences. | The L1 tests write each command reply and event sequence as golden JSON; L1 fails on a difference from the committed file; L2 replays only those files. |
| I-2 | Major | 40-41, 51, 77-79, 119 | L3 is the only proof of the wiring and is too slow to be required. | L1 writes a manifest of command names, argument keys and event names; L2 asserts that each name the UI uses is in it. |
| I-3 | Major | 65-67 | The guard accepts variables that anybody can set. | Drop `STORE_E2E_ISOLATED`; read the user from the OS, not from `%USERNAME%`; accept CI only with a second fact that a contributor desktop cannot have by accident. |
| I-4 | Major | 52, 92-97, 121, 129-130 | L4 is the only real-winget signal and is not required. | Run L4 in the required job only when the diff touches the executor; a timeout reports "inconclusive" and does not block. |
| I-5 | Nit | 16, 63-64, 106, 131-132 | Goal 1 promises pass or fail on any machine; `-Level e2e` outside an isolated host is only refused. | Exit code 2 and "skipped: not isolated" in the report. |
| I-6 | Nit | 88 | "No person present" against the approval of first-time contributors. | State that the approval is a security gate, not a test step. |
