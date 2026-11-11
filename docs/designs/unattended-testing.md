# Design — unattended testing for contributors (F2)

- **Status:** draft, not reviewed. Run `/egm:egm-review docs/designs/unattended-testing.md`
  before any code.
- **Date:** 2026-11-11
- **Issue:** F2 (GitHub #21). Relates to SPEC §8 and review action A20.

## Objective

Each test of the project runs with no person present. A contributor runs one command and gets
pass or fail. No test installs software on a contributor machine outside a sandbox or a CI
runner.

## Goals

1. One command for the fast tests, on any Windows machine with the toolchain.
2. A UI test level that runs with no window and no winget.
3. A real-winget test that can run only where it does no harm.
4. A pull request from a fork gets the same required check, with no secret.
5. A report that a person or a model can read: pass or fail per level, and the failed tests.

## Non-goals

- Pixel comparison of the UI.
- Tests of the Microsoft Store source, of elevation and of headless mode. Those work packages
  are not built.
- A test on Windows 10 or on Windows Home.
- Coverage targets.

## State today

- CI has one job, `windows`, on `windows-latest`: UI build, `cargo fmt --check`, clippy,
  build, `cargo test`. It has no path filter.
- Rust tests: unit tests, HTTP tests with `httpmock`, executor tests with the fake winget
  (`tests/fixtures/fake_winget.rs`), and 2 handler-to-database tests in
  `tests/install_loop.rs`.
- `tests/winget_smoke.rs` installs and removes `jqlang.jq` with the real winget. Its only
  guard is `#[ignore]`. **`cargo test -- --ignored` on a contributor machine installs
  software.** This breaks the requirement today.
- The UI has no test and no test tool. All backend calls go through one `call()` wrapper in
  `ui/src/lib/api.ts`.

## Design

### Test levels

| Level | What runs | Where | Gate |
|---|---|---|---|
| L1 Rust | `cargo fmt --check`, clippy, `cargo test` (unit, `httpmock`, fake winget, handler tests) | Any machine, CI | Required |
| L2 UI | Vitest, `@testing-library/svelte`, `mockIPC` from `@tauri-apps/api/mocks` | Any machine, CI | Required |
| L3 App smoke | The built app with the fake winget, driven through WebDriver (`@wdio/tauri-service`) | CI only at first | Not required |
| L4 Real winget | Install, check and remove one small package with the real winget | CI runner or Windows Sandbox only | Not required |

L1 and L2 use no network and no real winget. They are the required check.

### Answers to the five open questions

**1. Where do real installs run?**

- On the GitHub-hosted `windows-latest` runner. It is a new machine for each job and is free
  for a public repository.
- Optional local path: Windows Sandbox, started by a script. The script maps the repository
  read-only, maps a results folder, and runs the test from the sandbox logon command. It needs
  Windows Pro or higher and virtualization.
- **The guard is in the test code, not in the documentation.** The L4 test stops with a clear
  message unless one of these is true: `GITHUB_ACTIONS=true`; the user is `WDAGUtilityAccount`
  (the Windows Sandbox user); `STORE_E2E_ISOLATED=1`, which only the sandbox script sets.
- A disposable VM is not part of the design. It needs a large image and a license.

**2. How does a test drive the UI?**

- L2 is the required UI test. It mounts a component, mocks the IPC commands and events, and
  asserts what the user sees. Runs in seconds.
- L2 cannot find a difference between the mock and the real Rust reply. To limit that risk,
  one set of JSON fixture files defines the reply of each command. The Rust handler tests
  assert that the handlers produce those files. The UI mocks read the same files.
- L3 starts the real app with the fake winget and runs one journey: search, open detail,
  install, see "installed". It proves the wiring between the UI and Rust. It is slow and needs
  a matching Edge driver, so it is not required at first.

**3. How do fork pull requests run?**

- With the plain `pull_request` event. L1 and L2 need no secret. L4 needs only the public
  winget source and a public package, so it needs no secret also.
- Never use `pull_request_target` for a job that runs code from the pull request.
- A fork run cannot write a comment on the pull request. The report goes to the job summary
  and to an uploaded file.
- Repository setting: require approval for workflow runs from first-time contributors.

**4. Which checks gate a merge, and which run on a schedule?**

- Required: the `windows` job with L1 and L2. The job name stays `windows`.
- Not required, in a second workflow file: L3 and L4. Triggers: nightly schedule, manual
  start, and the label `run-e2e` on a pull request. Each job has a time limit, because a real
  install hung on a hosted runner in February 2026. L4 checks `winget --version` first and
  reports "skipped: winget absent" when the runner image has no winget.
- A nightly failure opens or updates one issue.
- The second file has no path filter, and it is not in branch protection. A required check
  that a path filter skips stays pending and blocks the merge.

**5. What is the one command, and what report does it write?**

- `scripts/test.ps1`, with one parameter:
  - `scripts/test.ps1` runs L1 and L2. This is the default.
  - `scripts/test.ps1 -Level smoke` adds L3.
  - `scripts/test.ps1 -Level e2e` runs L4, and only where the guard permits.
  - `scripts/test.ps1 -Sandbox` starts Windows Sandbox and runs L4 in it.
- Exit code 0 is pass. Exit code 1 is fail.
- Report: `target/test-report.md` with one line per level and the names of failed tests. In CI
  the same text goes to the job summary. JUnit XML from Vitest and from `cargo-nextest` is
  optional and comes later.
- CI calls the same script, so the contributor and CI run the same thing.

## Alternatives

| Alternative | Why not |
|---|---|
| `cargo xtask test` as the one command | A new crate for a one-person project. The app is Windows-only, so PowerShell is sufficient. Open it again if the script grows past about 150 lines. |
| WebDriver tests as the required UI test | Minutes per run, and a driver mismatch makes the run hang. It can come into the gate after it is stable for some weeks. |
| Playwright against the Vite dev server | It needs a browser download and proves nothing more than L2 for this UI. |
| Real winget in the required check | A third-party hang or a runner image change then blocks each merge. |
| Windows Sandbox in CI | No source shows that it starts on a hosted runner. |

## Risks

- **L2 passes while the real app is broken.** The shared fixtures and L3 limit this. Nothing
  removes it while L3 is not required.
- **A nightly job that nobody reads.** The automatic issue and the `run-e2e` label limit this.
- **The runner image is Windows Server 2025, not Windows 11.** winget is on the image but is
  not a listed tool. L4 proves less than a test on a desktop Windows.
- **The sandbox script needs a logged-in desktop session.** A contributor on a headless
  machine cannot use it.

## Work items, in order

1. Add the guard to `tests/winget_smoke.rs`. This closes the safety hole and is a small
   change. It can merge before the remaining items.
2. Add `scripts/test.ps1` with L1 only, and make CI call it.
3. Add Vitest and the first L2 tests: search results, the install button states, the error
   and cancel paths. Add the shared fixtures.
4. Add the second workflow with L4.
5. Add L3.
6. Add the sandbox script.
7. Replace SPEC §8 with a short text that points to this document.

## Open questions for the maintainer

1. Is `scripts/test.ps1` acceptable, or is one tool for each operating system (`cargo xtask`)
   a requirement?
2. Does L3 need to be in the first delivery, or can it follow?
3. Which small package does L4 install? `jqlang.jq` is the value in the smoke test today.

## Not verified

- Windows Sandbox on a hosted runner.
- That the Edge driver on the runner image matches its WebView2 runtime.
- That the sandbox runs for a user with no desktop session.
- The value of the first-time contributor approval setting in this repository.
