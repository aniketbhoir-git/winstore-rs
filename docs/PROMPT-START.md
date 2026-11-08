# Kickoff prompt — run inside this repo folder

Open Claude Code in the `store/` directory and paste the block below verbatim.
Fenced as plain text so it can be copied directly.

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

- Run it from the repo root so `docs/...` paths resolve.
- After the session, review the report, then issue a follow-up prompt for the next issue
  (recommended order in `docs/ISSUE-BACKLOG.md`: #1 → #2 → #3 ∥ #4 → #5 → #6).
- `CLAUDE.md` at the repo root auto-loads for every Claude Code session, so the guardrails
  stay in force even if a future prompt omits them.