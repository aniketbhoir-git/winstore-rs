# Stage 6 — Comprehension Test

- **Target:** `docs/designs/unattended-testing.md` (158 lines), commit `05d3f9d`. The reviewer
  read this file only.
- **Date:** 2026-11-12
- **Reviewer:** `egm-comprehension`, fresh context
- **Totals:** 0 Blocker, 4 Major, 9 Nit

## What this proposes (feature part, lines 1-133)

Every test should run with nobody watching, and nothing should install software on a
contributor's own machine. The design sets up four levels. L1 is the Rust checks and tests. L2
is UI component tests with mocked IPC. Both run anywhere, use no network and form the required
merge check. L3 starts the real app against a fake winget through WebDriver. L4 installs and
removes a real package with the real winget. L3 and L4 are optional and live in a second
workflow that runs nightly, on manual start, or when a pull request gets the `run-e2e` label.
L4 can only run where a guard in the test code allows it: GitHub Actions, the Windows Sandbox
user, or an env flag the sandbox script sets. One command, `scripts/test.ps1`, runs the levels,
writes a markdown report and sets the exit code, and CI calls the same script. Fork pull
requests use plain `pull_request` and need no secrets.

## How the existing system works, per this document, and the detailed part (lines 30-41, 134-158)

Today CI has one `windows` job that builds and runs fmt, clippy and `cargo test`.
`winget_smoke.rs` does real installs and is protected only by `#[ignore]`, so `--ignored`
installs software on the local machine. The UI has no tests, and every backend call goes
through `call()`. The work order is: add the guard first, then the script with L1, then L2 with
shared fixtures, then the L4 workflow, L3 and the sandbox script, and finally replace SPEC §8.
Three questions are still open: PowerShell versus `cargo xtask`, the timing of L3, and which
package L4 installs. Four claims are marked unverified.

The paraphrase is correct. The reader understood the design from the file alone.

## Findings

| Id | Sev | Lines | What the reader could not understand |
|---|---|---|---|
| C-1 | Major | 104-107, 51 | `-Level smoke` "adds L3" and `-Level e2e` "runs L4": does e2e run L1 and L2 also? How does `-Sandbox` combine with `-Level`? Line 51 says L3 is "CI only at first", and line 105 runs it locally. |
| C-2 | Major | 65, 96, 106, 108 | Is a guard stop or "skipped: winget absent" a pass or a fail? The report format does not show a skipped level. |
| C-3 | Major | 34, 51, 77 | "The fake winget" has no definition, and nothing says how the built app uses it in L3. |
| C-4 | Major | 74-76 | The fixtures have no location and no format. "Produce those files" can be an exact match or a schema match. Events are mocked, and the fixtures cover command replies only. |
| C-5 | Nit | 6, 144 | F2, #21, SPEC §8 and A20 have no explanation. |
| C-6 | Nit | 35, 49 | "Handler" has no definition. |
| C-7 | Nit | 92 | No reason for "the job name stays `windows`". |
| C-8 | Nit | 67 | "Which only the sandbox script sets" reads as enforced; it is a convention. |
| C-9 | Nit | 79, 156 | No reason why the Edge driver must match. |
| C-10 | Nit | 97 | Nothing says what opens the nightly issue. |
| C-11 | Nit | 25 | Three work packages are named and not described. |
| C-12 | Nit | 138-139 | No item adds L2 to the script and to the required check. |
| C-13 | Nit | 148-150 | Question 2 asks about L3, and the design includes it already. "One tool for each operating system" is unclear for a Windows-only app. |
