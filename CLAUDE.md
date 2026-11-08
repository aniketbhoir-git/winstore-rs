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