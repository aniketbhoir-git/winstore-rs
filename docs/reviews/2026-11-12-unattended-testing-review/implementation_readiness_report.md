# Stage 9 — Implementation Readiness Test

- **Target:** `docs/designs/unattended-testing.md`, DetailedRange lines 134-158, commit
  `05d3f9d`. Lines 1-133 are the reference.
- **Date:** 2026-11-12
- **Reviewers:** `egm-implementation-readiness`, `egm-codebase-details` (against branch
  `impl/6--install-loop`), `egm-style`. Each ran in a fresh context.
- **Raw totals:** 3 Blocker, 14 Major, 8 Nit

## Verdict

Not ready. Only work item 1 is near ready. Most of "State today" is correct against the code.

| Work item | Status | Main gap |
|---|---|---|
| 1 Guard | Ready, with one gap | "Stops" can be a pass, a panic or `#[ignore]`. |
| 2 `test.ps1` with L1 | Not ready | No workflow file, working directory, report line format or default. |
| 3 Vitest, L2, fixtures | Not ready | No fixture path, format or command list; no test cases; no step that adds L2 to the script and CI. |
| 4 L4 workflow | Not ready | No file name, schedule, time limit, issue mechanism, permissions. |
| 5 L3 | Not ready | Nothing says how the app uses the fake. Open question 2. |
| 6 Sandbox script | Not ready | No file name, no sandbox configuration, no way to get Rust and winget into the sandbox. |
| 7 SPEC §8 | Not ready | No statement of which part to replace and with what. |

## Readiness findings

| Id | Sev | Lines | Finding | Recommendation |
|---|---|---|---|---|
| R-2 | Blocker | 138-140, 103-107 | Item 2 builds the script "with L1 only". No later item adds L2, `-Level smoke`, `-Level e2e` or `-Sandbox`. | An explicit "extend `scripts/test.ps1`" step in items 3 to 6. |
| R-3 | Blocker | 139-140, 74-76 | The fixtures have no path, format, command list or named Rust test. The test cases are not listed, and the UI tests of SPEC §6.6 are not linked. | Name each. |
| R-7 | Blocker | 143, 61-64 | Item 6 has no script name and no sandbox configuration. A clean sandbox has no Rust toolchain and possibly no winget. | Specify a prebuilt test binary, the install steps, the mapped folders and the return of the exit code. |
| R-1 | Major | 134-145 | The design never uses the term "component-test level" that SPEC §7 WP5 step 3 waits for. | State in item 3 that L2 is that level and when it exists. |
| R-4 | Major | 136-145 | No item has an acceptance criterion that a command can decide. | One for each item. |
| R-5 | Major | 65-67, 136 | Item 1: pass, fail or skip? Does `#[ignore]` stay? How is the user read? | Specify. |
| R-6 | Major | 141, 93-97 | Item 4 has no values. | Name the file and give each value. |
| R-8 | Major | 142, 77-79 | Item 5: no switch for the fake, no WebDriver configuration. | Define them, or defer item 5. |
| R-9 | Major | 136-145 | Only item 1 states its dependencies. | A "depends on" note for each item. |
| R-10 | Major | 146-151 | Open question 1 can undo items 2 to 6. Question 3 blocks item 4, because no default is given. | Answer question 1 before item 2. Write the default package into item 4. |
| R-11 | Major | 144 | SPEC §8 also covers unit, HTTP and subprocess tests that this design does not describe. | Name the bullets to replace and the new text. |
| R-12 | Nit | 66 | The local runner `act` sets `GITHUB_ACTIONS=true`. | See D-1. |
| R-13 | Nit | 3 | The document is a draft. | Record the approval before item 3. |

## Codebase findings

| Id | Sev | Design lines | Code | Finding |
|---|---|---|---|---|
| CB-1 | Major | 35-36 | `src-tauri/src/ipc/tests.rs:20-300`, `ipc/test_support.rs:207-221` | The design counts 2 handler tests. `ipc/tests.rs` has about 10 more, with `FakeSource`, `FakeExec` and `RecordingSink`. The fixture assertions belong there. |
| CB-2 | Major | 51, 77-78 | `ipc/state.rs:81-82, 159`, `sources/winget_repo.rs:23, 88-89`, `config.rs:42-44` | The subprocess fake does not handle search or detail. The built app always calls the public catalog service and uses the real database and config of the user. L3 as written needs a code change or uses the network and the real profile. |
| CB-3 | Major | 74-76, 140 | `domain/progress.rs:81`, `executor/winget_cli.rs:587`, `services/install.rs:393, 400, 419` | Timestamps, log ids and the operation id of the real executor change in each run. A handler test cannot compare them to a fixed file. |
| CB-4 | Major | 52, 136 | `src-tauri/tests/winget_smoke.rs:35, 60-63` | The test asserts nothing without `STORE_SMOKE_ACCEPT=1`, an `agreements_required` stop counts as a pass, and no step checks that the package is installed. The guard must come before `WingetCli::connect` (line 39). |
| CB-5 | Major | 136 | `winget_smoke.rs:1-8, 33` | The module comment says "Never runs in CI" and "Requires the owner's approval". L4 in CI contradicts that. Item 1 must rewrite it. |
| CB-6 | Major | 51, 77 | `executor/winget_cli.rs:35, 80-92`, `tests/fixtures/fake_winget.rs:79-82` | A redirect exists already: `STORE_WINGET_PATH`, with `FAKE_WINGET_SCRIPT` for the fake. The design names none of the two. The fake reads one script file for each process. |
| CB-7 | Major | 50 | `ui/package.json:14-24`, `ui/vite.config.ts:5-10` | The UI uses Svelte 5.57, Vite 8.3 and TypeScript 6.0. No test tool, no DOM environment, no `test` script. The reviewer could not check that a Vitest release supports Vite 8. |
| CB-8 | Nit | 72 | `@tauri-apps/api/mocks.js` | A mocked `listen` needs `mockIPC(handler, { shouldMockEvents: true })`. |
| CB-9 | Nit | 75 | `domain/progress.rs:54` | `fraction` is an `f32`; a value such as 0.3 serializes with more digits. Use a tolerance or exact values. |
| CB-10 | Nit | 76 | `domain/package.rs:113`, `ipc/events.rs:80-112` | Test fixtures and an event recorder exist already. Build the JSON from them. |
| CB-11 | Nit | 109 | `.github/workflows/ci.yml:30-40` | No root workspace: "`target/`" is `src-tauri/target`. |
| CB-12 | Nit | 67 | `winget_smoke.rs:10-11` | `STORE_E2E_*` next to `STORE_SMOKE_*`: keep one prefix. |

Confirmed against the code: the one CI job and its steps; the test kinds and their files; the
2 tests in `install_loop.rs`; the smoke test, its package and its `#[ignore]` mark; no other
test starts the real winget; each `invoke` and `listen` call goes through
`ui/src/lib/api.ts`, so `mockIPC` can intercept them; `AppError` serializes with a fixed
shape; `scripts/` does not exist.

## Style findings (DetailedRange)

| Id | Sev | Lines | Finding |
|---|---|---|---|
| ST-2 | Major | 151 | "The smoke test today" is the L4 test; "smoke" is also the L3 level. |
| ST-9 | Nit | 148-150 | "One tool for each operating system" reads as the opposite of its meaning. "First delivery" has no definition against the work items. |
