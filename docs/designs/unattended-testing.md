# Design — unattended testing for contributors (F2)

Status: Approved 2026-11-25 — approved by owner through the project manager.

- **Date:** 2026-11-11. Revised and approved 2026-11-25.
- **Issue:** F2 (GitHub #21). Relates to SPEC §8, SPEC §7 WP5 step 3 and review action A20.
- **Review:** `docs/reviews/2026-11-12-unattended-testing-review/` (EGM, 2026-11-12). The
  action ids below (A1 to A12) come from `consolidated_review_report.md` in that folder.

## Review resolution

| Finding | Result | Where |
|---|---|---|
| A1 Make the guard safe | Resolved | "Isolation guard"; plan step 1 |
| A2 Decide the scope | Resolved: L3, Windows Sandbox, label and nightly automation are follow-ups | "Test levels", "Follow-up issues"; decision D1 |
| A3 Define the component-test level | Resolved | "UI test level (L2)"; plan step 3 |
| A4 Limit and define the shared fixtures | Resolved | "Shared fixtures"; plan step 4 |
| A5 Define the script and its outcomes | Resolved | "One command and report"; plan step 2 |
| A6 L4 policy | Resolved by move to the L4 follow-up issue | "Follow-up issues" item 1; decision D5 |
| A7 Close the hand-over from SPEC §8 | Resolved | "Hand-over from SPEC §8"; plan step 6 |
| A8 Make each work item decidable | Resolved | "Implementation plan" (each step has a check and a dependency) |
| A9 Repair the structure | Resolved | "Problem", design subsections, "Alternatives", "Risks" |
| A10 One term for one thing | Resolved | Whole document; "State today" has the correct test count |
| A11 Fork pull requests | Resolved | "Fork pull requests and merge gate"; plan steps 2 and 5 |
| A12 Trade-off for `cargo xtask`, source for the hang | Resolved: trade-off given, the unsourced sentence removed | "Alternatives" |
| I-1 Golden JSON for each command and event | Rejected (the review refused it) | A4 covers what L2 uses |
| I-2 Manifest of command and event names | Rejected (the review refused it) | Can come with L3 |
| I-4 L4 in the required job for executor diffs | Rejected (the review refused it) | "Alternatives" |

### Decisions taken on 2026-11-25

The owner approved the design and is not available for detail questions. For each choice of
taste, this revision takes the option that is easiest to reverse.

| Id | Decision | How to reverse |
|---|---|---|
| D1 | Accept the smaller scope of A2. L3, L4 in CI and the Windows Sandbox script become follow-up issues. | Specify a follow-up issue and build it. |
| D2 | The one command is `scripts/test.ps1`, not `cargo xtask` (design open question 1). | Replace one script file. CI calls only the script. |
| D3 | The script has no parameter. It runs L1 and L2. | Add a parameter later. |
| D4 | L2 uses Vitest with the `jsdom` DOM environment. | Change one line in the Vitest configuration to `happy-dom`. |
| D5 | The policy for agreements in L4 (A6, review decision 3) waits for the L4 follow-up issue. No test accepts an agreement until the owner decides. | Not needed. Nothing is built. |
| D6 | The L4 package stays `jqlang.jq` (design open question 3). | Change one default value. |
| D7 | The fork approval setting changes to "require approval for all external contributors". | One repository setting. |
| D8 | The shared fixtures are in `src-tauri/tests/fixtures/ipc/`, next to the subprocess fake. | Move one folder. |

## Problem

Requirement F2 (`docs/ISSUE-BACKLOG.md`, GitHub #21): all testing runs with no person present.
A contributor runs one command and gets pass or fail. This includes contributors who use
cheaper models. SPEC §8 gives the details of the levels, the guard and the open checks to this
design.

Two gaps exist today:

1. One test can install software on a contributor machine (see "State today").
2. The UI has no test. SPEC §7 WP5 step 3 cannot start until a component-test level exists.

## Objective

Each test of the project runs with no person present. A contributor runs one command and gets
pass or fail. No test installs or removes software on a contributor machine. Only a
GitHub-hosted runner can run a real install.

## Goals

1. One command for the required tests, on any Windows machine with the toolchain.
2. A UI test level that runs with no window and no winget. This is the component-test level
   of SPEC §7 WP5 step 3.
3. A real-winget test that refuses to run on any machine except a GitHub-hosted runner.
4. A pull request from a fork gets the same required check, with no secret.
5. A report that a person or a model can read: pass or fail per level, and the failed tests.

## Non-goals

- Pixel comparison of the UI.
- Tests of the Microsoft Store source, of elevation and of headless mode. Those work packages
  are not built.
- A test on Windows 10 or on Windows Home.
- Coverage targets.
- Paid services and telemetry. Each tool in this design is free and sends no usage data.
- The five UI tests of SPEC §6.6. They belong to WP5 step 3. If F2 holds them, F2 and WP5
  step 3 wait for each other.

## State today

- CI has one job, `windows`, on `windows-latest`: UI build, `cargo fmt --check`, clippy,
  build, `cargo test`. It has no path filter. The workflow has no `permissions` key. The
  repository default for the workflow token is `read`.
- Rust tests: unit tests, HTTP tests with `httpmock`, executor tests with the *subprocess
  fake* (the cargo example `fake_winget`, `src-tauri/tests/fixtures/fake_winget.rs`), about 15
  handler tests with `FakeExec`, `FakeSource` and `RecordingSink` in
  `src-tauri/src/ipc/tests.rs`, and 2 handler-to-database tests in
  `src-tauri/tests/install_loop.rs`.
- `src-tauri/tests/winget_smoke.rs` installs and removes `jqlang.jq` with the real winget. Its
  only guard is `#[ignore]`. `cargo test -- --ignored` on a contributor machine installs
  software. This breaks the requirement today.
- The UI has no test and no test tool. All backend calls go through one `call()` wrapper in
  `ui/src/lib/api.ts`.
- The fork approval setting of the repository is `first_time_contributors` (read with
  `gh api` on 2026-11-25).

## Design

### Test levels

| Level | What runs | Where | Gate |
|---|---|---|---|
| L1 Rust | `cargo fmt --check`, clippy, `cargo test` (unit, `httpmock`, subprocess fake, `FakeExec` handler tests, shared fixture checks) | Any machine, CI | Required |
| L2 UI | Vitest, `jsdom`, `@testing-library/svelte`, `mockIPC` from `@tauri-apps/api/mocks` | Any machine, CI | Required |
| L4 Real winget | `src-tauri/tests/winget_smoke.rs` | GitHub-hosted runner only | Not required; follow-up issue |

L1 and L2 use no network and no real winget. They are the required check. L3 (the built app
through WebDriver) is a follow-up issue. The id L3 stays reserved for it.

### Isolation guard

The guard is in the test code, not in the documentation.

- The L4 test runs only when both conditions are true: `GITHUB_ACTIONS` is `true`, and
  `RUNNER_ENVIRONMENT` is `github-hosted`. GitHub documents both variables. A self-hosted
  runner sets `RUNNER_ENVIRONMENT` to `self-hosted`, so a runner on a person's machine fails
  the check.
- The check runs before `WingetCli::connect`. So a refused run starts no winget process.
- A refusal fails the test. The message says: "This test installs software. It runs only on a
  GitHub-hosted runner." The message names no variable.
- `#[ignore]` stays. `cargo test` and `scripts/test.ps1` never run the test.
- The check is a pure function that takes a lookup for environment values. A unit test
  checks it with no real environment.
- The module comment of the test file changes to this rule.
- There is no other way past the guard. The old draft had `STORE_E2E_ISOLATED` and a
  `WDAGUtilityAccount` branch. This design removes both.

Remaining risk: a person can set the two variables by hand. This is a deliberate act, not an
accident. "Risks" accepts it.

### UI test level (L2)

- L2 is the component-test level of SPEC §7 WP5 step 3.
- Tools: Vitest, the `jsdom` DOM environment, `@testing-library/svelte`, and `mockIPC` from
  `@tauri-apps/api/mocks`. SPEC §2 lists them as dev dependencies of `ui/`.
- A test mounts a component, mocks the IPC commands, and asserts what the user sees.
- A mocked `listen` needs `mockIPC(handler, { shouldMockEvents: true })`. A test then sends an
  event with `emit`.
- A test records each mocked call in an array. "No call" is an assertion that the array has
  no entry for that command.
- **Exit criterion.** L2 exists when the required `windows` job runs these three tests and
  they pass:
  1. A component calls a mocked command and shows its reply.
  2. A component shows the result of an emitted `store:progress` event.
  3. A user action that must not call the backend makes no call.
- When L2 exists, WP5 step 3 can start.

### Shared fixtures

L2 cannot find a difference between the mock and the real Rust reply. Shared fixtures limit
that risk. They cover only the payloads that the L2 tests use.

- Folder: `src-tauri/tests/fixtures/ipc/`. Format: one JSON file per payload, pretty-printed,
  UTF-8.
- Files:
  - `error_agreements_required.json`: the serialized `AppError` for `agreements_required`.
  - `event_progress.json`: one `store:progress` payload.
  - `event_operation_finished.json`: one `store:operation-finished` payload.
  - `event_search_results.json`: one `store:search-results` payload.
- Source: the test data that exists (`domain::package::fixtures`, `RecordingSink`,
  `FakeExec`).
- Rust side: tests in `src-tauri/src/ipc/tests.rs` build each payload, normalize it, and
  compare it with the file as a `serde_json::Value`.
- Normalization: operation ids, log ids and timestamps get fixed values before the compare.
  `fraction` values are exact in binary (for example 0.25 and 0.5).
- UI side: the L2 mocks import the same files.

### One command and report

- Command: `scripts/test.ps1`, with no parameter. It runs L1 and L2. It works in Windows
  PowerShell 5.1 and in PowerShell 7.
- Order: UI build, L2 (`npm test` in `ui/`), then L1 (`cargo fmt --all -- --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build --locked`,
  `cargo test --locked` in `src-tauri/`). The script runs `npm ci` only when
  `ui/node_modules` is absent.
- The script continues after a failed level, so the report shows each level.
- Report: `src-tauri/target/test-report.md`. One line per level, in this format:
  - `L1 Rust: pass (N passed, M ignored)`
  - `L2 UI: fail` and below it one line for each failed test name.
- Outcomes:
  - Exit code 0: each level passed.
  - Exit code 1: a level failed, or a tool is absent. The report names the absent tool (for
    example `L2 UI: fail (npm not found)`).
  - An ignored test is not a failure. The report shows the count.
  - The script never runs ignored tests. So the guard of L4 never runs from the script.
- In CI the script also writes the report text to the job summary (`GITHUB_STEP_SUMMARY`).
- CI calls the same script, so a contributor and CI run the same thing.
- JUnit XML output is not part of this design.

### Fork pull requests and merge gate

- The `windows` job uses the plain `pull_request` event. L1 and L2 need no secret.
- The workflow sets `permissions: contents: read`.
- Never use `pull_request_target` for a job that runs code from a pull request.
- A fork run cannot write a comment. The report goes to the job summary.
- Security gate, not a test step: the repository setting "require approval for all external
  contributors" (`all_external_contributors`). Today the value is `first_time_contributors`.
- Required check: the `windows` job with L1 and L2. The job name stays `windows`.
- No required check has a path filter. A required check that a path filter skips stays
  pending and blocks the merge.

### Hand-over from SPEC §8

SPEC §8 gives four open checks to this design.

| Check | Level | Owner |
|---|---|---|
| Elevation prompt | Deferred. The elevation work package is not built. Its issue adds the test. | Elevation work package |
| Cancel during a download | L1 today: cancel with the subprocess fake. A cancel with a real download is deferred to the L4 follow-up issue. | Follow-up issue 1 |
| Offline behavior | L1: the `httpmock` tests of catalog errors and the adapter timeout test. L2 in WP5 step 3: the UI error state. | F2 (L1), WP5 step 3 (L2) |
| No network call outside hard rule 5 | Deferred. Until then, guardrail item 3 of SPEC §9 (review of new URLs) covers it. | Follow-up issue 4 |

Plan step 6 replaces three bullets of SPEC §8: "Real winget (CI or a sandbox only)", "No
required manual step" and "Checks with no automated test yet". The new text says:

- Real winget: only on a GitHub-hosted runner. The guard in `tests/winget_smoke.rs` refuses
  each other machine. See `docs/designs/unattended-testing.md`.
- Required tests: L1 and L2, through `scripts/test.ps1`. No required manual step.
- Open checks: the table in "Hand-over from SPEC §8" of the design.

The bullets for unit, HTTP, subprocess and in-process tests stay as they are.

## Alternatives

| Alternative | Why not |
|---|---|
| `cargo xtask test` as the one command | It adds a crate and a build before each test run. The app runs only on Windows, so a PowerShell script needs no build and no new dependency. The cost: a contributor must read PowerShell. Open it again if the script grows past about 150 lines. |
| WebDriver tests as the required UI test | Minutes per run, and a driver mismatch makes the run hang. They can come into the gate after L3 is stable for some weeks. |
| Playwright against the Vite dev server | It needs a browser download and proves nothing more than L2 for this UI. |
| Real winget in the required check (also only for executor diffs, I-4) | A third-party hang or a runner image change then blocks each merge. |
| Windows Sandbox in CI | No source shows that it starts on a hosted runner. |
| Windows Sandbox script for local L4 | A clean sandbox has no Rust toolchain and possibly no winget. It needs a build pipeline. Follow-up issue 3. |
| A disposable VM | It needs a large image and a license. |
| A free environment variable that permits L4 locally | A person or a model can set it by accident. The guard accepts only the GitHub-hosted runner. |
| The `run-e2e` label and a nightly run | With a label, code from a fork can reach a real install. A nightly run needs `issues: write`. A manual start from the base repository has neither problem. Follow-up issue 1. |

## Risks

| Risk | Mitigation |
|---|---|
| L2 passes while the real app is broken. | The shared fixtures bind the payloads. L3 (follow-up issue 2) removes more of the risk. Accepted until then. |
| A person sets `GITHUB_ACTIONS` and `RUNNER_ENVIRONMENT` by hand and runs `--ignored`. | Accepted. It is a deliberate act. `#[ignore]`, the refusal message and the module comment warn first. |
| Until follow-up issue 1, no automated test runs the real winget. | Accepted. The executor tests with the subprocess fake cover the parser and the state flow. |
| A Vitest or `jsdom` release does not support Svelte 5 runes or Vite 8. | Plan step 3 checks it first. Vitest 5.1 declares Vite 8 as a supported peer. |
| `mockIPC` needs browser objects that `jsdom` lacks. | Plan step 3 adds a small setup file. If that fails, switch to `happy-dom` (decision D4). |

## Implementation plan

Each step is one pull request. Each step has a check that a command can decide. A step
starts when its dependencies are merged.

1. **Guard the real-winget test.** Add the guard function and its unit test. Call it first in
   `src-tauri/tests/winget_smoke.rs`, before `WingetCli::connect`. Rewrite the module comment.
   Keep `#[ignore]`.
   - Depends on: nothing.
   - Check: `cargo test --test winget_smoke` passes the guard unit test. On a machine with no
     `RUNNER_ENVIRONMENT=github-hosted`, `cargo test --test winget_smoke -- --ignored` fails
     with the refusal message and starts no winget process. Run this check only after the
     guard is in the code.
2. **Add `scripts/test.ps1` with L1.** Add the report, the exit codes and the job summary.
   Change `.github/workflows/ci.yml`: the `windows` job calls the script, and the workflow sets
   `permissions: contents: read`.
   - Depends on: step 1.
   - Check: `powershell -File scripts/test.ps1` exits 0 on a clean checkout and writes
     `src-tauri/target/test-report.md` with an `L1 Rust: pass` line. A test that fails on
     purpose gives exit 1 and its name in the report. The `windows` check passes on the pull
     request.
3. **Add the L2 harness.** Add Vitest, `jsdom`, `@testing-library/svelte` and a `test` script
   to `ui/package.json`. Add the Vitest configuration and a setup file. Add the three exit
   criterion tests. Add L2 to `scripts/test.ps1`. List the dev dependencies in SPEC §2.
   - Depends on: step 2.
   - Check: `npm test` in `ui/` runs three tests and passes. The report has an `L2 UI: pass`
     line. The `windows` check runs L2. After the merge, WP5 step 3 can start.
4. **Add the shared fixtures.** Add the four JSON files, the compare tests in
   `src-tauri/src/ipc/tests.rs`, and the import of the files in the L2 mocks.
   - Depends on: step 3.
   - Check: `cargo test` and `npm test` pass. A change to one field name in
     `event_progress.json` makes `cargo test` fail.
5. **Set the fork approval setting.** The maintainer changes the setting to
   `all_external_contributors` (Settings, Actions, General, or `gh api`).
   - Depends on: nothing. Needs repository admin rights.
   - Check: `gh api repos/aniketbhoir-git/winstore-rs/actions/permissions/fork-pr-contributor-approval`
     returns `all_external_contributors`.
6. **Update SPEC §8 and `CLAUDE.md`.** Replace the three bullets of SPEC §8 with the text in
   "Hand-over from SPEC §8". Change the F2 status in `CLAUDE.md` and
   `docs/ISSUE-BACKLOG.md`.
   - Depends on: step 4.
   - Check: SPEC §8 contains `scripts/test.ps1` and no longer contains "it is not approved
     yet".
7. **Open the follow-up issues.** Open one GitHub issue for each item of "Follow-up issues",
   with the notes there. Each title starts with "F2 follow-up:". Link each one from F2.
   - Depends on: nothing.
   - Check: `gh issue list --search "F2 follow-up"` lists four issues.

## Follow-up issues

These are not part of this design. Each one needs its own design before code.

1. **L4 in CI, manual start only.** A second workflow file with `workflow_dispatch` only, no
   label, no schedule, a time limit per job, and `permissions: contents: read`. The test first
   checks `winget --version` and fails with "winget absent" when it is missing. Notes for
   that design: the smoke test asserts nothing without `STORE_SMOKE_ACCEPT=1`; an
   `agreements_required` stop counts as a pass today; no step checks that the package is
   installed. It needs an "is installed" check. **Needs the owner:** can a CI test give consent
   to an agreement for one operation, under hard rule 4? This joins spike question 2 of SPEC
   §6.6 and F4. Also covers a cancel during a real download.
2. **L3 app smoke.** The built app with the subprocess fake through WebDriver. Notes: the app
   always calls the public catalog service and uses the real profile; `STORE_WINGET_PATH`
   with `FAKE_WINGET_SCRIPT` exists already; an override of the executor path must be limited
   to a test build; the Edge driver must match WebView2.
3. **Windows Sandbox script for local L4.** Notes: the host must build the test binary; the
   sandbox needs winget; a read-only mapping cannot hold `target/`; it needs a desktop
   session. The guard then needs a sandbox condition that no free variable can fake.
4. **Network check for hard rule 5.** A test that proves no call goes outside the permitted
   destinations.

## Not verified

- That `mockIPC` works in `jsdom` with no extra setup (plan step 3 checks it).
- That Vitest 5.1 runs Svelte 5 runes components with `@sveltejs/vite-plugin-svelte` 7 (the
  peer range permits Vite 8; plan step 3 checks it).
- The items of the follow-up issues: Windows Sandbox on a hosted runner, the Edge driver
  against WebView2, the sandbox with no desktop session, winget on the runner image.

Verified on 2026-11-25: GitHub documents `GITHUB_ACTIONS` and `RUNNER_ENVIRONMENT` (values
`github-hosted` and `self-hosted`). The repository is public, so hosted runners are free.
