# Session prompts

Paste-ready prompts for a Claude Code (or other model) session in the `store/` repo root.
Fenced as plain text so they can be copied directly.

## Status (2026-11-11)

- Issues #1 to #5 are merged on `main`. #6 (MVP) is built on `impl/6--install-loop`, PR #19,
  not merged.
- `docs/SPEC.md` was reviewed with the Elephant-Goldfish Model: 6 blockers. See
  `docs/reviews/2026-11-11-spec-review.md`.
- Manual test of the #6 build: the app starts and search runs, but search returns unrelated
  results (F1). Testing must become unattended for contributors (F2).
- Repo: https://github.com/aniketbhoir-git/winstore-rs — issues numbered as in
  `docs/ISSUE-BACKLOG.md`; `main` is protected and requires the `windows` CI check.
- Next: the Elephant session below. The one-issue resume prompt stays valid for coding work
  after the spec blockers are closed.

## Elephant session prompt — design and orchestration (Fable)

The Elephant holds the project memory and makes no large code change itself. Reviewers
("Goldfish") run in fresh contexts through the `egm` plugin. Run from the `store/` folder.

```text
You are the Elephant for "Store" (repo aniketbhoir-git/winstore-rs): the architect and
orchestrator session. You keep the project context, decide the order of work, write and fix
design documents, and hand coding work to subagents. Development on this project now follows
the Elephant-Goldfish Model (EGM, the `egm` plugin).

READ FIRST, IN THIS ORDER:
1. CLAUDE.md
2. docs/reviews/2026-11-11-spec-review.md (the verdict, the six blockers, the conflict table,
   the suggested order)
3. docs/ISSUE-BACKLOG.md, section "Review follow-ups (2026-11-11)" (F1, F2, F3)
4. docs/SPEC.md and docs/STRATEGY.md
Then run `git status` and `git log --oneline -5`. The working branch is
impl/6--install-loop (PR #19, open). Run `cargo test` in src-tauri once for a baseline.

STATE:
- WP0 to WP4 are on main. WP5 (issue #6) is built on impl/6--install-loop and not merged.
- docs/SPEC.md has 6 blockers and does not show what is built. Where SPEC.md and the code
  disagree, the code is the fact and the spec needs the edit. Do not rebuild code.
- Bug F1: search for "vlc" shows unrelated apps. Cause: the api.winget.run service returns
  unrelated, stale rows. The adapter passes the query correctly.
- Requirement F2: all testing runs unattended, and a contributor can run it with one command.

GOALS FOR THIS SESSION, IN ORDER:
1. Spec repair. Apply review actions A1, A3, A2, A4 to docs/SPEC.md: fix the update-check
   text, add a status per work package and record the WP5 commands that exist, define the
   agreement consent model, add the state transition table. Follow the review's conflict
   table where reviewers disagree. Keep CLI flags in the winget-executor skill, not the spec.
2. F1. Write a decision record in docs/adr/ for the community catalog backend. Compare
   `winget search`/`winget show` through the existing executor, the official winget source
   index, and another REST mirror. After the maintainer picks one, plan the fix as an issue.
3. F2. Draft a design document in docs/designs/ for automated, unattended testing. Answer the
   five open questions in F2. A test must never install software on a contributor machine
   outside a sandbox or a CI runner.

HOW TO WORK:
- EGM skills are user-invoked. When a step needs one, name the command and ask the
  maintainer to run it: /egm:egm-brainstorm, /egm:egm-draft, /egm:egm-review <doc>,
  /egm:egm-drift-report <dir>. Ask for /egm:egm-review on each design document and on the
  repaired SPEC.md before any code starts.
- Stress-test each idea before you agree with it. Lead with what is wrong or missing.
- Hand coding and long research to subagents with a narrow brief and a capped report.
  Do short reads yourself.
- One decision per question to the maintainer. Proceed on all work that does not depend on
  the answer.
- Docs commits use the `docs:` prefix. Do not push, merge PR #19, or create GitHub issues
  without the maintainer's word.

NON-NEGOTIABLE RULES (docs/SPEC.md §0): public, documented interfaces only (winget, MSIX,
Windows Deployment API); no payment, account sign-in or licensing logic; a paid item shows
only the "Buy in Microsoft Store" browser hand-off; no telemetry; original branding.

FIRST REPLY: a status board (what is done, what is blocked, what you start first) and the
one decision you need from the maintainer, if any. Then start goal 1.
```

## Resume prompt — one issue per session

Replace `<N>` with the issue number. Run from the `store/` folder.

```text
You are continuing "Store" (working title; repo aniketbhoir-git/winstore-rs), an open-source
Rust + Tauri 2 app storefront and package-manager front end for Windows. Issues #1 to #4
are done on main: the crate in src-tauri/ builds, 149 tests pass, CI is green.

YOUR SCOPE (hard stop): implement Issue #<N> exactly as written in docs/ISSUE-BACKLOG.md and
the matching GitHub issue. Do not start any other issue. Stop and report when its acceptance
criteria are met.

BEFORE WRITING CODE:
1. Read CLAUDE.md, then docs/SPEC.md §0 and the sections the issue cites, then the issue.
2. Read the existing code you build on: src-tauri/src/{lib.rs,error.rs,config.rs},
   src-tauri/src/domain/*.rs, src-tauri/src/db/{mod.rs,models.rs,repo/*.rs}.
3. Run `cargo test` in src-tauri once to confirm the baseline is green before you change
   anything. Cargo lives at %USERPROFILE%\.cargo\bin (see CLAUDE.md "Environment notes").

WORKFLOW:
- Branch from main: impl/<N>--short-name. Commit logical chunks with clear messages.
- After each chunk: cargo build -> cargo test -> cargo clippy --all-targets -- -D warnings
  -> cargo fmt. Zero warnings.
- Tests for parsers and state transitions. Mock HTTP with httpmock; mock winget with a fake
  executable fixture. Never call the network or winget in unit tests.
- When done: push the branch, open a PR with the repository template, make sure the
  `windows` CI check passes, and tick the guardrail checklist in the PR body.

NON-NEGOTIABLE RULES (docs/SPEC.md §0): positive, neutral terminology; public, documented
interfaces only (winget, MSIX, Windows Deployment API); no payment, account sign-in, or
licensing/entitlement logic ever (paid items show only the "Buy in Microsoft Store" browser
hand-off); no telemetry beyond the catalogs the user enables; MIT; original branding.

STACK: Rust stable, Tauri 2, Svelte/TypeScript, sqlx SQLite, tokio, windows-rs. You may add
reqwest + httpmock (issue #3) or tokio-process helpers (issue #4). Ask before any other dep.

ESCALATION: if a decision changes scope, needs an unlisted dependency, or conflicts with
SPEC.md, stop and report the question with the options. Aniket decides, or hands it to the
Fable orchestrator session.

END WITH A SHORT REPORT: files touched, test/clippy output tail, PR link, deviations from
the spec, open questions.
```

## Kickoff prompt (historical — used for #1 and #2)

```text
You are bootstrapping "Store" (working title) — an open-source Rust + Tauri 2 app
storefront and package-manager front end for Windows. This is a new repo that currently
contains docs only; your job this session is to turn it into a compiling, tested skeleton.

SCOPE FOR THIS SESSION (hard stop):
Implement Issue #1 (project scaffold + CI) and Issue #2 (domain model + persistence) from
docs/ISSUE-BACKLOG.md. Then stop and report. Do NOT begin Issue #3 or beyond without my
explicit approval.

BEFORE WRITING CODE:
1. Read docs/STRATEGY.md (context), then docs/SPEC.md in full (source of truth), then
   docs/ISSUE-BACKLOG.md.
2. Verify the toolchain and report what's missing instead of improvising installs:
   - cargo --version, rustc --version
   - node --version (needed for the Tauri frontend)
   - cl.exe / MSVC Build Tools present (Windows build)

NON-NEGOTIABLE RULES (from docs/SPEC.md §0):
- Positive, neutral terminology. Stay strictly within public, documented interfaces
  (winget, MSIX, Windows Deployment API).
- No payment, account sign-in, or licensing/entitlement logic — ever. A paid app shows only
  a "Buy in Microsoft Store" browser hand-off.
- No telemetry beyond the catalogs the user enables.
- MIT license; original branding only — no Microsoft marks or assets.

ENGINEERING STANDARDS:
- Stack exactly per docs/SPEC.md §2: Rust stable, Tauri 2, Svelte/TypeScript, SQLite via
  sqlx, tokio, windows-rs. Do not introduce heavy unlisted dependencies.
- Work incrementally: implement -> cargo build -> cargo test -> cargo clippy -> cargo fmt.
  Commit logical chunks with clear messages.
- Tests for parsers and state transitions; migrations must apply from an empty database.

DEFINITION OF DONE:
- Issue #1 acceptance criteria fully met (build + tests green on Windows, window opens,
  config round-trips, migrations apply).
- Issue #2 acceptance criteria fully met (domain types, schema, repositories, mapping tests).
- End with a short report: what you built, test/clippy output, any toolchain gaps, which
  issue you recommend next (#3 or #4), and any deviations from the spec.
```

## Notes

- Run prompts from the repo root so `docs/...` paths resolve.
- `CLAUDE.md` at the repo root auto-loads for every Claude Code session, so the guardrails
  stay in force even if a prompt omits them.
- Recommended order: #1 → #2 → (#3 ∥ #4) → #5 → #6 (MVP); everything after #6 is parallel.
