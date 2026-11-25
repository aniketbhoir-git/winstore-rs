# CLAUDE.md

Open-source Rust + Tauri 2 app storefront and package-manager front end for Windows
("Store", working title). MIT licensed, community-built, slow-moving.

## Source of truth (read before coding)
- `docs/SPEC.md` — implementation spec (interfaces, modules, work packages)
- `docs/STRATEGY.md` — rationale and scope
- `docs/ISSUE-BACKLOG.md` — work items and their acceptance criteria
- `docs/reviews/2026-11-11-spec-review.md` — open defects in SPEC.md (actions A1 to A25)

## Non-negotiable rules
- Positive, neutral terminology. Stay strictly within public, documented interfaces
  (winget, MSIX, Windows Deployment API).
- No payment, account, or licensing/entitlement logic — ever. A paid app shows only a
  "Buy in Microsoft Store" button that opens the official page in the browser.
- No telemetry beyond the catalogs the user explicitly enables.
- Original branding only; do not bundle Microsoft marks or assets.

## Stack (do not add heavy dependencies beyond SPEC.md §2)
Rust stable · Tauri 2 · Svelte/TypeScript · SQLite (sqlx) · tokio · windows-rs

## Work loop
- Work one issue at a time, in backlog order unless told otherwise.
- Implement incrementally: `cargo build` → `cargo test` → `cargo clippy` → `cargo fmt`
  after each logical chunk; commit frequently with clear messages.
- Write tests for parsers and state transitions; migrations must apply from an empty DB.
- Stop at the issue's definition of done, then report before continuing.

## Tier note
This environment runs `tier:max` issues (winget parser, install loop, msstore adapter,
elevation). `tier:any` issues are reserved for community contributors on cheaper models.

## Status (2026-11-25)
- On `main`: #1 scaffold + CI, #2 domain model + persistence, #3 community catalog, #4 winget
  executor + progress parser, #5 IPC layer + UI skeleton, #6 install loop (core and UI), F4
  agreement consent (#45). Catalog design (`docs/designs/`): steps 1 to 3 and 5 to 7 built;
  step 4 is owner-gated. Testing design (F2): steps 1 to 4 and 6 built. #12 (hardening, toasts,
  export/import, update scan): steps 1 to 6 built; step 7 waits for WP6, step 8 for the owner.
- Open owner decisions: catalog step 4 and the ADR 0001 confirmation; ADR 0002 (PR #47); the
  #33 agreement decision; the #12 questions in `docs/designs/hardening.md`.
- `docs/SPEC.md` was repaired and reviewed with EGM (reports in
  `docs/reviews/2026-11-11-spec-review-2/`). A section marked *built* follows the code; a
  *planned* section overrides it. Where a built section and the code disagree, stop and
  report; do not rebuild code that exists. Open review actions are in
  `docs/reviews/2026-11-11-spec-review.md`.
- Requirement: all testing runs unattended, for contributors also. Run
  `powershell -File scripts/test.ps1` (L1 Rust + L2 UI; report in `src-tauri/target/test-report.md`).
  The real-winget test refuses to run off a GitHub-hosted runner; no other test may call
  `WingetCli::connect`. Design: `docs/designs/unattended-testing.md`. F2 steps 5 (fork
  approval setting) and 7 (follow-up issues) are open.
- Known bug: search returns unrelated results (the winget.run service does; F1, #20, #16).
  Catalog step 4 (wire the winget CLI source) fixes it; it waits for the owner.
- Next: the "Review follow-ups" section of `docs/ISSUE-BACKLOG.md`.

## Design workflow — Elephant-Goldfish Model (EGM)
- Design decisions go through the `egm` plugin. Its skills are user-invoked: name the command
  and ask the maintainer to run it. Do not start them yourself.
  `/egm:egm-brainstorm`, `/egm:egm-draft`, `/egm:egm-review <doc>`, `/egm:egm-drift-report <dir>`.
- A feature with a design document or a multi-step plan gets a review before code. A bug fix
  or a small change does not.
- Approved design documents go in `docs/designs/`. Review results go in `docs/reviews/`.
  Decision records go in `docs/adr/`.
- Repo: https://github.com/aniketbhoir-git/winstore-rs — GitHub issue numbers match
  `docs/ISSUE-BACKLOG.md`. Session prompts live in `docs/PROMPT-START.md`.

## Git workflow
- `main` is protected: the `windows` CI check must pass; no force pushes.
- Work on `impl/<issue#>--short-name`, open a PR with the template, tick the guardrail
  checklist. Commit messages: conventional prefix (`feat(core):`, `fix(db):`, `ci:`, `docs:`).

## Environment notes (maintainer machine, Windows 11)
- Cargo is at `%USERPROFILE%\.cargo\bin` and may be missing from a tool's PATH. Bash:
  `export PATH="$HOME/.cargo/bin:$PATH"`; PowerShell: `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`.
- Toolchain: Rust stable MSVC, VS Build Tools 2022 (C++ workload), Node 22+, WebView2.
- Build the UI once (`npm install && npm run build` in `ui/`) before the first `cargo build`;
  `tauri::generate_context!` embeds `ui/dist`.
- Bitdefender may lock a freshly built `src-tauri/target/debug/store.exe` on first launch
  and then break the linker (LNK1104). The `src-tauri/target` folder is excepted; if the
  lock returns, `cargo test --lib` still works, and a reboot clears it.
- Window smoke test: start `target\debug\store.exe`, poll `MainWindowTitle` for "Store".

## Layout (as built)
- `src-tauri/src/error.rs` — `AppError` (+ JSON `code/message/detail/retryable`), `AppResult`.
- `src-tauri/src/config.rs` — `AppConfig` JSON file (no agreement setting; a flag goes to winget only for one consented operation).
- `src-tauri/src/domain/{package,source,install,progress}.rs` — SPEC §4 types; `string_enum!`
  macro in `domain/mod.rs` gives Display/FromStr/serde for every enum.
- `src-tauri/src/db/{mod,models}.rs`, `db/repo/*.rs`, `db/migrations/*.sql` — pool, row
  mapping, repositories (packages with TTL, installs with guarded state transitions,
  sources, settings, operation_log).
- `src-tauri/src/catalog/{mod,search}.rs`, `sources/winget_repo.rs` — `CatalogSource` trait,
  search coordinator with TTL cache (`detail_fetched_at`), winget.run v2 adapter (reqwest, httpmock tests).
- `src-tauri/src/executor/{mod,winget_cli,winget_parse}.rs`, `tests/fixtures/fake_winget.rs` —
  `PackageExecutor` trait, winget subprocess wrapper (the stop function is a field, `StopFn`, for tests), progress parser; the fake is a cargo example.
- `src-tauri/src/ipc/{commands,events,state}.rs` — Tauri commands (`search_sources`, `get_app_detail`, `start_install`, `cancel_install`, `check_updates`, `update_all`, `open_store_page`, `start_server` stub), `store:*` events, lazy `AppState`/`Core`; handlers are plain async fns tested in `ipc/tests.rs`.
- `src-tauri/src/services/{install,consent}.rs` — install lifecycle; `consent.rs` holds the in-memory agreement challenges and checks a consent against them.
- `src-tauri/src/services/{export,import,notify,schedule}.rs` — export/import of winget JSON (paths checked as strings first), toast decisions, the in-app update scan timer.
- `src-tauri/src/platform/{mod,windows}.rs` — Windows seams behind traits: `Notifier`, window focus, elevation check (fakes for tests).
- `ui/src/{lib,components,routes}` — typed API client + event subscriptions, `Search.svelte` (debounced), `AppCard`, `InstallButton`, `ConsentDialog` (agreement consent, F4), `ProgressBar`, `SourceBadge`, `DetailActions`, `DetailHeader`, `AdminBanner`, `LogTail`, `ExportPanel`, `ImportPanel`, `UpdateScanSetting`, `ui/src/routes/AppDetail.svelte`.

## Project skills
- `winget-executor` — winget CLI facts, output/exit-code parsing, cancellation, fake-winget tests for #4 and #6 — `.claude/skills/winget-executor/SKILL.md`
- `store-work-resume` — load context and follow the workflow to resume one issue ("start issue N") — `.claude/skills/store-work-resume/SKILL.md`
