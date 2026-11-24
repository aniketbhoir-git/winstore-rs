# Stage 7 — Critic Review

- **Target:** `docs/designs/unattended-testing.md`, FeatureRange lines 1-133, commit `05d3f9d`
- **Date:** 2026-11-12
- **Reviewers:** `egm-design`, `egm-structural`, `egm-style`, each in a fresh context. The style
  reviewer covered the two ranges in one pass; its items for lines 134-158 are in the Stage 9
  report.
- **Raw totals:** 1 Blocker, 17 Major, 9 Nit

## Verdict

Revise. The guard, the fixture contract and the scope of L2 do not yet do what the design
claims, and SPEC §7 WP5 step 3 depends on the three.

## Design findings

| Id | Sev | Lines | Finding | Recommendation |
|---|---|---|---|---|
| D-1 | Blocker | 65-67 | `GITHUB_ACTIONS=true` can be set by a shell, by the local runner `act`, or by a self-hosted runner on a person's machine. `STORE_E2E_ISOLATED=1` is a convention only. | Require `GITHUB_ACTIONS=true` with `RUNNER_ENVIRONMENT=github-hosted`. For the sandbox path, require the `WDAGUtilityAccount` user check and no free variable. |
| D-2 | Major | 65-66, 95-96, 108 | A refused guard and "skipped: winget absent" return early from a Rust test, and `cargo test` counts that as a pass. The nightly run can stay green after the runner image loses winget. | A separate result for a refusal and a skip in the report and the exit code. |
| D-3 | Major | 74-76 | The fixtures cover command replies only. Events and the serialized `AppError` (with the dialog data of `agreements_required`) are not bound, and the argument names that `api.ts` sends are not bound. | Fixtures for each event payload and each error code, with normalized ids and timestamps. A check that `types.ts` agrees, or generate it. |
| D-4 | Major | 72-73 | Nothing maps L2 to the five UI tests of SPEC §6.6. The design gives no criterion for "the component-test level exists". | Name the event option of `mockIPC`, say how a test records "no call", and define "exists". |
| D-5 | Major | 93-97 | The `run-e2e` trigger has no definition. With `labeled` and `synchronize`, a fork author can push new code after the label, and that code runs a real install. | `types: [labeled]` only, remove the label after each run, `permissions: contents: read`, a new label for each head commit. |
| D-6 | Major | 97 | The nightly issue needs `issues: write` in the workflow file that fork pull requests also start. No rule for duplicates or for closing. | The permission on one job that runs only on `schedule`; a lookup key and a close rule. |
| D-7 | Major | 52, 58-60 | On a new runner, winget asks for the source agreement. The smoke test asserts nothing without `STORE_SMOKE_ACCEPT=1`, so L4 can pass with no install. SPEC §6.6 requires a rewrite of this test to the consent model. | Define the L4 assertions and how the test gives a consent for one operation under hard rule 4. |
| D-8 | Major | 62-64 | Windows Sandbox has no winget and no Rust toolchain. A read-only mapping cannot hold `target/`. | Say if the host builds the test binary, and how winget gets into the sandbox. Without that the sandbox path cannot work. |
| D-9 | Major | 77-78 | Nothing says how the built app is pointed at the fake. A variable that redirects the executor to any `.exe` is an attack surface in a release build. | Limit the override to a test build. |
| D-10 | Nit | 51 | `@wdio/tauri-service` is not in "Not verified". | Add it, or name `tauri-driver`. |
| D-11 | Nit | 95 | The hang "in February 2026" has no source. | Link it. |
| D-12 | Nit | 118 | "A one-person project" contradicts "community-built" in CLAUDE.md. | Give a concrete trade-off. |
| D-13 | Nit | 88 | The default approval setting covers only accounts that are new to GitHub. | Require the stricter setting and move it into the design. |

## Structural findings

| Id | Sev | Lines | Finding | Recommendation |
|---|---|---|---|---|
| S-1 | Major | 56-112 | "Answers to the five open questions" holds the core design, and the document never lists the five questions. | Design subsections: isolation guard, UI test strategy, fork pull requests, merge gates and schedules, one command and report. |
| S-2 | Major | 8-41 | No problem section. Line 39 says "the requirement" and does not name it. | A "Problem" section that cites F2 and SPEC §8. |
| S-3 | Major | 51, 77-79, 105 | L3 traces to no goal. | A goal for the UI-to-Rust wiring, or move L3 to a later phase. |
| S-4 | Major | 22-28 | The four open checks of SPEC §8: only elevation has a place. | A goal, a non-goal or an open question for each. |
| S-5 | Major | 124-132 | Two of four risks have no mitigation. The guard bypass and the driver mismatch are not in the list. | A mitigation or "accepted, because" for each; add the two risks. |
| S-6 | Nit | 68, 85 | Two rejected options are inside the design text. | Move them to the alternatives table. |
| S-7 | Nit | 58-101 | The sub-topics are bold paragraphs, not headings. | Headings. |

## Style findings (FeatureRange)

| Id | Sev | Lines | Finding | Recommendation |
|---|---|---|---|---|
| ST-1 | Major | 34, 49, 51, 77 | "The fake winget" covers the *subprocess fake* and `FakeExec`, which SPEC §8 separates. | Use the two SPEC terms. |
| ST-2 | Major | 37, 51-52, 66, 93, 105-106 | "Smoke" names L3 and also the L4 test file. "e2e" names L4, and the `run-e2e` label also starts L3. | One name for each level, used in the parameter, the label, the variable and the file name. |
| ST-3 | Major | 103-107 | "One parameter" and a second one. It is not clear which levels `e2e` runs. | List each switch and its exact set of levels. |
| ST-4 | Major | 65-67, 96, 106, 108 | Exit codes 0 and 1 only; a refusal and a skip have no outcome. | Give the outcome, exit code and report line. |
| ST-5 | Major | 67 | "Which only the sandbox script sets" cannot be enforced. | Say that the variable is trusted, or remove it. |
| ST-6 | Nit | 11, 62, 107, 131, 157 | "Sandbox" is general in line 11 and Windows Sandbox elsewhere. Line 131 states as fact what line 157 lists as not verified. | "Windows Sandbox" where that is meant. |
| ST-7 | Nit | 74-76 | The fixtures have no path, no format and no command list. | Name them. |
| ST-8 | Nit | 88, 93-97 | No actor for the label, no value for the time limit, no title for the issue. | Give each. |
