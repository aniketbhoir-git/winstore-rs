# Stage 8 — Intermediate Zoom Out Review

- **Target:** `docs/designs/unattended-testing.md` (158 lines), commit `05d3f9d`, not changed
  since Stage 5. The flow ran with no fixes between stages.
- **Date:** 2026-11-12
- **Reviewer:** `egm-zoom-out`, fresh context, with the four other stage reports as input
- **Totals:** 0 Blocker, 3 Major, 2 Nit

## Verdict

Revise toward a smaller design. The other stages recommend about 60 changes. If the author
applies all of them, the design becomes larger than requirement F2 needs: a second test
harness (L3), a build pipeline for Windows Sandbox, label and issue automation, and a contract
for each command and event.

## Conflicts between recommendations

| Conflict | The simpler side |
|---|---|
| Z-2, S-3 "drop L3" against D-9, CB-2, R-8, D-10 "specify L3" | Drop L3; a follow-up issue holds D-9, CB-2 and R-8 as notes. |
| Z-3 "drop the sandbox script" against D-8, R-7 "specify it" | Drop the script, `-Sandbox`, and the `WDAGUtilityAccount` branch (IZ-1). |
| I-4 "L4 in the required job" against design line 121 | L4 stays out of the required check. |
| I-1, I-2, D-3 "golden files for each command and event, a manifest" against Z-8 | Z-8: fixtures only for the payloads that the L2 tests use, built from the test data that exists, with normalized ids and timestamps. |
| I-5, D-2, ST-4 "exit code 2 and a skip outcome" against exit 0 and 1 | Keep 0 and 1. A guard refusal fails the test with a message. `#[ignore]` stays. |
| D-5 label protocol, D-6 issue automation, Z-7 | Manual start only (`workflow_dispatch`), in a follow-up issue. |
| Z-11, ST-3, C-1 "repair `-Level`" | No parameter. The script runs L1 and L2. |
| Z-6, D-7, CB-4 "L4 agreement policy" | Move to the L4 follow-up issue; it overlaps the consent rewrite of SPEC §6.6. |

## Recommendations to refuse as scope growth

I-1, I-2, I-4, I-5, D-3 (the generated `types.ts`), D-5, D-6, D-8, R-7, D-9, CB-2, R-8, S-3,
R-6, ST-8, C-10, D-2, ST-4, and R-2 for each level after L2. For Z-5 and S-4: one line for
each open check, "deferred, issue #n", and no mechanism.

## The smallest design that meets F2

1. **The guard** in `src-tauri/tests/winget_smoke.rs`: `GITHUB_ACTIONS=true` and
   `RUNNER_ENVIRONMENT=github-hosted`, before `WingetCli::connect`. If not, the test fails
   with a message that names no variable. `#[ignore]` stays; the module comment changes.
2. **`scripts/test.ps1`**, no parameter: L1 and L2, a report file, exit 0 or 1; the `windows`
   job calls it.
3. **The L2 harness:** Vitest, a named DOM environment, `@testing-library/svelte`, listed in
   SPEC §2; `mockIPC` with `shouldMockEvents: true`. The level exists when the required job
   runs three tests: a component with a mocked command, an emitted event, and an assertion
   that no call was made. The five UI tests of SPEC §6.6 belong to WP5 step 3.
4. **Limited shared fixtures:** the serialized `AppError` (with `agreements_required`), the
   progress and finished event payloads, the search reply. `ipc/tests.rs` asserts them; the L2
   mocks read them.
5. **Fork safety and the SPEC:** the plain `pull_request` event, workflow
   `permissions: contents: read`, the stricter approval setting. Replace only two bullets of
   SPEC §8.

L3, L4 in CI and the sandbox become follow-up issues.

## New findings

| Id | Sev | Finding |
|---|---|---|
| IZ-1 | Major | Z-3 keeps the `WDAGUtilityAccount` branch of the guard. With no script, nothing can run the test in Windows Sandbox. The branch is then dead code and a second way past the guard. |
| IZ-2 | Major | The `run-e2e` label is the only path on which code from a fork reaches a real install. A manual start from the base repository removes D-5 and D-6. |
| IZ-3 | Major | Z-4, D-4 and R-3 put the five UI tests of SPEC §6.6 into F2. SPEC §7 WP5 step 3 waits for F2, so each one then waits for the other. The exit criterion of F2 must be what the harness can do. |
| IZ-4 | Nit | With no parameter, open question 1 and the "150 lines" rule lose most of their weight. |
| IZ-5 | Nit | Lines 83-84 argue that L4 is safe for forks. With a manual start only, delete the argument. |
