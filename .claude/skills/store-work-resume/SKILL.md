---
name: store-work-resume
description: Load the right context to resume work on one Store issue (repo aniketbhoir-git/winstore-rs). Use when the user says "resume", "pick up issue N", "continue Store", "start issue N", or opens a fresh session in the Store repo to do a backlog item.
disable-model-invocation: false
---

# Resume work on one Store issue

One issue per session. Replace `N` with the issue number. Run from the repo root
(`D:\vaults\Terminus\MSStore-Rust\store`). Related prompt text: `docs/PROMPT-START.md`.

## 1. Reading order (do not skip, do not glob around)

| # | Read | Why |
|---|---|---|
| 1 | `CLAUDE.md` | Rules, stack, work loop, git workflow, environment notes, Layout |
| 2 | `docs/SPEC.md` §0 | Terminology and the four hard rules |
| 3 | The SPEC sections the issue cites | Interfaces and data flows (for example §5.2, §6.2 for #4) |
| 4 | `docs/ISSUE-BACKLOG.md` entry for N | Goal, files, requirements, acceptance, dependencies |
| 5 | `gh issue view N --repo aniketbhoir-git/winstore-rs` | The live issue; it wins over the backlog text if they differ, then report the difference |
| 6 | Modules named in CLAUDE.md "Layout" that the issue builds on | `error.rs`, `config.rs`, `domain/*.rs`, `db/{mod,models}.rs`, `db/repo/*.rs`, `lib.rs` |
| 7 | Any project skill that matches (see CLAUDE.md "Project skills") | For #4 and #6 use `winget-executor` |

Check dependencies first: every issue listed under "Depends on" must be merged on `main`
(`git log --oneline -10`, `gh pr list --repo aniketbhoir-git/winstore-rs --state merged`).
If one is missing, stop and report.

## 2. Baseline

Confirm the tree is green before changing anything.

```bash
export PATH="$HOME/.cargo/bin:$PATH"      # bash; cargo may be missing from PATH
cd src-tauri && cargo test
```

PowerShell: `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.
First build needs `npm install && npm run build` in `ui/` (the Tauri context embeds `ui/dist`).
If the linker fails with LNK1104 (Bitdefender lock), use `cargo test --lib`; a reboot clears it.
Record the baseline test count in your report (58 after #2).

## 3. Branch and PR workflow (CLAUDE.md "Git workflow")

- Branch from an up-to-date `main`: `impl/N--short-name`.
- After each logical chunk: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`. Zero warnings. Commit with a conventional prefix (`feat(core):`, `fix(db):`, `ci:`, `docs:`).
- Tests for parsers and state transitions. Mock HTTP with httpmock; mock winget with the fake fixture. Never hit the network or winget in unit tests.
- Finish: push, open a PR with the repository template, tick the guardrail checklist (SPEC §9), wait for the `windows` CI check. `main` is protected: no force pushes.
- Stop at the issue's acceptance criteria. Do not continue into the next issue.

## 4. Escalation: stop and report

Stop and report (question, options, minimum facts) when any of these occur:

- The change would alter scope or touch another issue's files.
- A dependency not allowed by SPEC §2 or the issue text is needed (only `reqwest` + `httpmock` for #3, tokio process helpers for #4 are pre-approved).
- The issue or code conflicts with SPEC.md.
- An approval-gated action is needed (merge, release, secrets).

Do not guess and continue. Aniket decides, or passes it to the orchestrator session.

## 5. Report format at the end of the session

```text
Issue: #N <title>        Branch: impl/N--name        PR: <url>
Files touched: <paths>
Tests: <before> -> <after>; clippy/fmt clean: yes/no (paste last 3 lines of cargo test)
CI: windows check <status>
Deviations from SPEC/issue: <none | list>
Open questions: <none | list>
Recommended next: <issue>
```

Keep it under 20 lines. Paths, not diffs.

## 6. Do not

- No payment, account sign-in, or licensing/entitlement code. Paid items only get the "Buy in Microsoft Store" browser hand-off.
- No telemetry or network calls beyond catalogs the user enabled.
- No dependencies outside SPEC §2 without asking.
- No Microsoft marks or assets; use plain "winget" and neutral terms (SPEC §0).
- Never silently pre-accept agreements; every mutating action starts from a user click.
- Never start a different issue, even a small adjacent fix. Note it in the report instead.
- No force pushes, no direct commits to `main`.
