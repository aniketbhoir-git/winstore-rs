# Consolidated review — unattended testing design

- **Target:** `docs/designs/unattended-testing.md` (158 lines), commit `05d3f9d`
- **Date:** 2026-11-12
- **Mode:** consolidated report, all review rubrics, no stop between stages
- **Stage reports** (this folder): `zoom_out_report.md`, `comprehension_report.md`,
  `critic_report.md`, `intermediate_zoom_out_report.md`, `implementation_readiness_report.md`
- **Raw totals over 5 stages:** 4 Blocker, 48 Major, 35 Nit, from 9 reviewer runs
- **After the merge:** 3 Blocker, 8 Major, 1 Nit action items; 3 recommendations refused

## Verdict

Not ready for code, except work item 1 (the guard), which is ready after action A1.

What is wrong, in order of weight:

1. **The guard does not guard.** One environment variable that a person or a model can set
   starts a real install on a contributor machine. Five reviewers found this.
2. **The design is too large for its requirement.** L3, the Windows Sandbox script and the
   label and nightly automation are about half of the document. None of them is necessary for
   F2, each one has facts that nobody checked, and two of them cannot work as written.
3. **The design does not say what SPEC §7 WP5 step 3 waits for.** The term "component-test
   level" is not in the document, and it has no exit criterion.
4. **Work items 2 to 7 cannot start** without questions to the author.

What holds: the description of the state today is correct against the code, with one
exception (the count of handler tests). The comprehension reviewer paraphrased the design
correctly from the file alone. The choice of L2 with `mockIPC` fits the code: each `invoke`
and `listen` call goes through `ui/src/lib/api.ts`.

**This design is a draft that this session wrote.** The gaps above are gaps in that draft.

## Action items

### 🔴 Blocker

**A1 — Make the guard safe.**

- Require `GITHUB_ACTIONS=true` together with `RUNNER_ENVIRONMENT=github-hosted`.
- Remove `STORE_E2E_ISOLATED`. Remove the `WDAGUtilityAccount` branch if A2 removes the
  sandbox script.
- Put the check before `WingetCli::connect`. A refusal fails the test with a message that
  names no variable. `#[ignore]` stays. Rewrite the module comment of the test file.
- Not verified: the reviewers named `RUNNER_ENVIRONMENT` from memory. Check the GitHub
  documentation before the code.

**A2 — Decide the scope.** This is a maintainer decision.

- The proposal of Stage 8: remove L3, the Windows Sandbox script, the `run-e2e` label, the
  nightly run and the automatic issue from this design. Each becomes a follow-up issue.
  L4 in CI starts by hand only (`workflow_dispatch`), also in a follow-up issue.
- Result: until that follow-up, no automated test runs the real winget. The guard makes the
  smoke test refuse on each machine that is not a hosted runner.
- The other side: keep them and specify them (D-5, D-6, D-8, D-9, R-6, R-7, R-8, CB-2). That
  is about 8 more Major items and 4 facts that are not verified.

**A3 — Define the component-test level and when it exists.**

- L2 is the component-test level of SPEC §7 WP5 step 3.
- It exists when the required `windows` job runs three tests: a component with a mocked
  command, an emitted event (`mockIPC` with `shouldMockEvents: true`), and an assertion that
  no call was made.
- The five UI tests of SPEC §6.6 belong to WP5 step 3, not to F2. If F2 holds them, F2 and
  WP5 step 3 wait for each other.
- Name the DOM environment and list the dev dependencies in SPEC §2. Check that a Vitest
  release supports Vite 8 and Svelte 5 runes; add it to "Not verified" until then.

### 🟡 Major

**A4 — Limit and define the shared fixtures.** Cover only the payloads that the L2 tests use:
the serialized `AppError` (with `agreements_required`), the progress and finished events, the
search reply. Give the folder and the format. Build them from the test data that exists
(`domain::package::fixtures`, `RecordingSink`, `FakeExec`). Assert them in `ipc/tests.rs`.
Normalize ids and timestamps; use `fraction` values that are exact in binary.

**A5 — Define the script and its outcomes.** With A2: no parameter, L1 and L2, the report at
`src-tauri/target/test-report.md`, exit 0 or 1. State what a refused or skipped test gives.
Say which work item adds L2 to the script and to the required job.

**A6 — L4 policy.** The smoke test asserts nothing without `STORE_SMOKE_ACCEPT=1`, and an
`agreements_required` stop counts as a pass. State how L4 gives a consent for one operation
under hard rule 4, and add an "is installed" check. With A2 this goes into the L4 follow-up
issue, with spike question 2 of SPEC §6.6.

**A7 — Close the hand-over from SPEC §8.** SPEC §8 gives four checks to this design:
elevation prompt, cancel during a download, offline behavior, no network call outside hard
rule 5. Give each one line: a level, or "deferred" with an owner. Name the bullets of SPEC §8
that work item 7 replaces, and the new text.

**A8 — Make each work item decidable.** One acceptance criterion that a command can decide,
and a "depends on" note, for each item. Answer the three open questions or give a default;
question 1 can undo items 2 to 6.

**A9 — Repair the structure.** A "Problem" section that cites F2 and SPEC §8. Design
subsections in place of "Answers to the five open questions", whose questions the document
never lists. A mitigation or "accepted, because" for each risk; add the guard bypass. Move
the two rejected options in the design text into the alternatives table.

**A10 — One term for one thing.** *Subprocess fake* and `FakeExec` as in SPEC §8, not "the
fake winget". One name for each level in the parameter, the variable and the file name
("smoke" names L3 and the L4 test file today). "Windows Sandbox" where that is meant. Paths
from the repository root. Correct the count of handler tests (`ipc/tests.rs` has about 10).

**A11 — Fork pull requests.** The plain `pull_request` event and
`permissions: contents: read`. The stricter approval setting, stated as a security gate and
not as a test step. Read the present value of the setting.

### 🟢 Nit

**A12.** The reason against `cargo xtask` must be a trade-off, not "a one-person project".
Give a source for the hang "in February 2026", or remove the sentence.

## Refused recommendations

| Id | Recommendation | Reason |
|---|---|---|
| I-1 | Golden JSON for each command reply and each event sequence | A contract for the whole IPC surface; A4 covers what L2 uses. |
| I-2 | A manifest of command and event names that L2 checks | A new mechanism; it can come with L3. |
| I-4 | L4 in the required job when the diff touches the executor | A third-party hang then blocks a merge; the design rejects that with a reason. |

## Traceability

| Action | Raw findings |
|---|---|
| A1 | Z-1, I-3, D-1, ST-5, C-8, R-5, R-12, CB-5, CB-12, IZ-1 |
| A2 | Z-2, Z-3, Z-7, S-3, D-5, D-6, D-8, D-9, D-10, D-11, R-6, R-7, R-8, CB-2, CB-6, C-9, C-10, ST-8, IZ-2, IZ-5 |
| A3 | Z-4, Z-9, D-4, R-1, R-3 (test list), CB-7, CB-8, IZ-3 |
| A4 | Z-8, D-3, C-4, ST-7, R-3 (fixtures), CB-1, CB-3, CB-9, CB-10 |
| A5 | Z-11, I-5, C-1, C-2, C-12, D-2, ST-3, ST-4, R-2, CB-11, IZ-4 |
| A6 | Z-6, D-7, CB-4 |
| A7 | Z-5, S-4, R-11 |
| A8 | R-4, R-9, R-10, R-13, C-13, ST-9 |
| A9 | S-1, S-2, S-5, S-6, S-7, C-5, C-6, C-7, C-11 |
| A10 | Z-10, ST-1, ST-2, ST-6, C-3, CB-1 |
| A11 | D-13, I-6 |
| A12 | D-11, D-12 |
| Refused | I-1, I-2, I-4 |

## Decisions that the maintainer must make

1. **A2:** remove L3, the sandbox script and the label and nightly automation from this
   design, or keep them.
2. **Open question 1 of the design:** `scripts/test.ps1` or `cargo xtask`. With A2 the script
   has no parameter and is short.
3. **A6:** can a CI test accept an agreement with a flag, with no person? Hard rule 4 says
   the user must see the agreement. A test has no user.
