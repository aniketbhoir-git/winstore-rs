# CLAUDE.md

Open-source Rust + Tauri 2 app storefront and package-manager front end for Windows
("Store", working title). MIT licensed, community-built, slow-moving.

## Source of truth (read before coding)
- `docs/SPEC.md` — implementation spec (interfaces, modules, work packages)
- `docs/STRATEGY.md` — rationale and scope
- `docs/ISSUE-BACKLOG.md` — work items and their acceptance criteria

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

## Status
- Done on `main`: #1 scaffold + CI, #2 domain model + persistence (58 tests, clippy/fmt clean).
- Next: #3 (community catalog) and #4 (winget executor) in parallel; then #5.
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
- `src-tauri/src/config.rs` — `AppConfig` JSON file; `accept_source_agreements` defaults false.
- `src-tauri/src/domain/{package,source,install,progress}.rs` — SPEC §4 types; `string_enum!`
  macro in `domain/mod.rs` gives Display/FromStr/serde for every enum.
- `src-tauri/src/db/{mod,models}.rs`, `db/repo/*.rs`, `db/migrations/*.sql` — pool, row
  mapping, repositories (packages with TTL, installs with guarded state transitions,
  sources, settings, operation_log).
- `ui/` — Vite + Svelte + TS skeleton; `invoke('app_version')` is the only command so far.

## Project skills
- `winget-executor` — winget CLI facts, output/exit-code parsing, cancellation, fake-winget tests for #4 and #6 — `.claude/skills/winget-executor/SKILL.md`
- `store-work-resume` — load context and follow the workflow to resume one issue ("start issue N") — `.claude/skills/store-work-resume/SKILL.md`
