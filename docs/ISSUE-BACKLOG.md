# Issue backlog

Tracked on GitHub: https://github.com/aniketbhoir-git/winstore-rs/issues (same numbers).

Each issue below is a **self-contained, paste-ready prompt**. Copy the body into your model
together with `docs/SPEC.md` §0 (terminology + hard rules).

## Labels

- `tier:max` — needs a top-tier model (Claude Max/Ultra, Sonnet 4.5+, Opus, GPT-5 class). Maintainer-owned by default.
- `tier:any` — any decent model (Haiku 4.5, DeepSeek V3.2, Gemini 2.5 Flash, Qwen3). Community token-friendly.
- `needs-windows-test` — must be validated on a real Windows box before merge.
- Size: `S` < 1 day of focused work · `M` 1–3 days · `L` up to a week.

## Dependency map

```
#1 scaffold+CI ──┐
#2 domain+DB   ──┼─► #3 community catalog ─┐
                 └─► #4 winget executor   ─┼─► #5 IPC+UI skeleton ─► #6 detail+install (MVP)
                                                                        │
                ┌───────────────────────────────────────────────────────┼─────────────────────────┐
                ▼              ▼               ▼              ▼          ▼                ▼
              #7 library   #8 msstore    #9 polish    #10 scoop/choco   #11 headless   #12 hardening
              +updates     +hand-off                   (after #3,#4)    server mode
```

Suggested order: **#1 → #2 → (#3 ∥ #4) → #5 → #6 (MVP)** — everything after #6 is parallelizable.

---

### Issue 1 — Project scaffold + CI (SPEC WP0) — ✅ done
- **Labels:** `tier:any`, `size:S`, `area:infra`
- **Depends on:** none
- **Goal:** a compilable Rust + Tauri 2 repo that builds and tests on Windows CI.
- **Files:** create `src-tauri/Cargo.toml`, `tauri.conf.json`, `src/main.rs`, `src/lib.rs`,
  `src/error.rs`, `src/config.rs`, `src/db/mod.rs` + empty migrations dir, `ui/` skeleton,
  `.github/workflows/ci.yml` (windows-latest: build, `cargo test`, clippy, fmt).
- **Requirements:** error enum from SPEC §1.3 with JSON `code`/`message`/`detail`/`retryable`;
  config load/save with defaults; SQLite pool init + migrations table. Window opens.
- **Acceptance:** `cargo build` + `cargo test` green on Windows; a window opens; config
  round-trips; migrations apply from empty.
- **LLM note:** `tier:any`; pure scaffolding, cheap model is fine.

### Issue 2 — Domain model + persistence (SPEC WP1) — ✅ done
- **Labels:** `tier:any`, `size:S`, `area:core`
- **Depends on:** #1
- **Goal:** the §4 enums/structs and the §4.3 SQLite schema, with repositories.
- **Files:** `src/domain/*.rs`, `src/db/migrations/*.sql`, `src/db/models.rs`.
- **Requirements:** implement exactly the fields in SPEC §4; row ↔ domain mapping; repositories
  for `packages` (cache TTL), `installs`, `sources`, `settings`, `operation_log`. No business
  logic yet.
- **Acceptance:** unit tests pass for mapping + migrations; a temp DB applies cleanly from empty.
- **LLM note:** `tier:any`.

### Issue 3 — Community catalog source (SPEC WP2, §5.1)
- **Labels:** `tier:any`, `size:M`, `area:catalog`
- **Depends on:** #1, #2
- **Goal:** search + detail over the winget community catalog, served as typed domain objects.
- **Files:** `src/catalog/mod.rs`, `src/catalog/search.rs`, `src/sources/winget_repo.rs`.
- **Requirements:** `CatalogSource` trait (search/get_detail/capabilities) from SPEC §5.1; JSON
  catalog fetch via `reqwest` with timeout, retry/backoff, and a proper User-Agent; parse into
  `PackageSummary`/`PackageDetail`; cache results (TTL) through the repositories.
- **Acceptance:** an `httpmock` integration test returns typed summaries for a query without
  invoking winget; malformed/rate-limited responses map to clean errors.
- **LLM note:** `tier:any`; mostly HTTP/JSON.

### Issue 4 — winget executor + progress parser (SPEC WP3, §5.2, §6.2)
- **Labels:** `tier:max`, `size:L`, `area:executor`, `needs-windows-test`
- **Depends on:** #1, #2
- **Goal:** the subprocess layer that actually installs/removes/updates.
- **Files:** `src/executor/mod.rs`, `src/executor/winget_cli.rs`, fake-winget fixture for tests.
- **Requirements:** discover winget path + version (`is_available()`); install/remove/update
  wrappers; streaming stdout/stderr parser producing `ProgressEvent` stages; cancellation via a
  cancel token + process kill; surface source/package agreements (never silent). Executor trait
  from SPEC §5.2.
- **Acceptance:** tests drive install/update/remove against the fake and parse every stage +
  error path; a manual smoke test installs one small real package on Windows.
- **LLM note:** `tier:max` — this is the hardest, most safety-critical piece.

### Issue 5 — Tauri IPC layer + UI skeleton (SPEC WP4, §5.3, §6.1)
- **Labels:** `tier:any`, `size:M`, `area:ui`
- **Depends on:** #3, #4
- **Goal:** the typed command/event surface plus a minimal search page.
- **Files:** `src-tauri/src/ipc/commands.rs`, `events.rs`, `state.rs`; `ui/src/lib/*`,
  `ui/src/routes/Search.svelte`, components (`AppCard`, `InstallButton`, `ProgressBar`,
  `SourceBadge`).
- **Requirements:** implement every command/event named in SPEC §3 `ipc/`; search calls the
  coordinator (§5.3) and streams results; debounced input; loading/empty/error states.
- **Acceptance:** search from the UI returns real community results; an install button emits
  progress events end-to-end.
- **LLM note:** `tier:any` (boilerplate + Svelte).

### Issue 6 — App detail + full install loop — **MVP** (SPEC WP5, §6.2/§6.3/§6.4)
- **Labels:** `tier:max`, `size:L`, `area:core`, `needs-windows-test`
- **Depends on:** #5
- **Goal:** the complete, reliable install/uninstall/cancel loop from the UI.
- **Files:** `src-tauri/src/services/install.rs`, detail route + components, log tail pane.
- **Requirements:** detail page (screenshots, description, versions, rating); start/ cancel/
  uninstall via the executor; `InstallState` transitions persisted; purchase hand-off button for
  `is_paid` (opens official page in browser — nothing else).
- **Acceptance:** end-to-end install/uninstall/cancel of a community app from the UI works
  repeatedly; a cancelled install cleans up state. **This closes the MVP.**
- **LLM note:** `tier:max` (glues async streams through IPC).

### Issue 7 — Library + updates (SPEC WP6, §6.3)
- **Labels:** `tier:any`, `size:M`, `area:core`, `needs-windows-test`
- **Depends on:** #6
- **Goal:** installed-app list, update detection, update one/all.
- **Files:** `src-tauri/src/services/library.rs`, `upgrade.rs`, Library + Updates routes.
- **Requirements:** installed list from executor `list_installed()`; diff vs catalog → badges;
  `update_one`/`update_all` sequential (concurrency 1) with progress; pinning persists.
- **Acceptance:** scan finds an outdated throwaway package; update-all converges; pinned items
  are skipped.
- **LLM note:** `tier:any` (reuses the #4 parser).

### Issue 8 — msstore source + purchase hand-off (SPEC WP7, §6.4)
- **Labels:** `tier:max`, `size:M`, `area:catalog`, `needs-windows-test`
- **Depends on:** #6
- **Goal:** the Store-catalog (`msstore`) source for free apps, plus paid-item detection.
- **Files:** `src-tauri/src/sources/msstore.rs`.
- **Requirements:** adapter behind the same `CatalogSource` trait; resolve free items to package
  metadata; `is_paid` flag drives the browser hand-off only. Treat as best-effort with clean
  fallback when unavailable.
- **Acceptance:** free Store apps resolvable; a paid item shows only the browser hand-off, no
  install path.
- **LLM note:** `tier:max` (payload shape needs care; stay exactly within SPEC §0).

### Issue 9 — Polish: categories, featured, theming, a11y (SPEC WP8)
- **Labels:** `tier:any`, `size:M`, `area:ui`
- **Depends on:** #6
- **Goal:** store-quality look and feel.
- **Files:** theme CSS, `Category`/`Featured` routes, `RatingStars`, `UpdateRow`, empty/error
  states.
- **Requirements:** category browsing + featured carousel; screenshots/ratings caching; dark/
  light theme; keyboard navigation and focus states.
- **Acceptance:** visual parity checklist passes; tab/keyboard navigation works end-to-end.
- **LLM note:** `tier:any`.

### Issue 10 — Scoop + Chocolatey sources (SPEC WP9)
- **Labels:** `tier:any`, `size:M`, `area:catalog`, `needs-windows-test`
- **Depends on:** #3, #4
- **Goal:** two more sources behind the same traits.
- **Files:** `src-tauri/src/sources/scoop.rs`, `choco.rs`; `src-tauri/src/executor/scoop_cli.rs`,
  `choco_cli.rs`.
- **Requirements:** catalog + executor for each; source picker in the UI; explicit user consent
  per source before first use.
- **Acceptance:** a Scoop-only app and a Chocolatey-only app each install from their own source.
- **LLM note:** `tier:any` (pattern is established by #3/#4).

### Issue 11 — Headless / home-server mode (SPEC WP10, §6.5)
- **Labels:** `tier:any`, `size:L`, `area:server`
- **Depends on:** #6
- **Goal:** manage a headless Windows Server box from another machine's browser.
- **Files:** `src-tauri/src/server/mod.rs`, `api.rs`, `state.rs` (feature-gated `server`);
  minimal web UI reusing the same components.
- **Requirements:** axum server on localhost (LAN + token only with explicit opt-in); JSON routes
  + SSE log stream over the identical core services, no duplicated logic.
- **Acceptance:** install/list/update a package on Windows Server 2022 from a second machine.
- **LLM note:** `tier:any` (standard web server).

### Issue 12 — Hardening: elevation, toasts, scheduled scan, import/export (SPEC WP11)
- **Labels:** `tier:max`, `size:L`, `area:platform`, `needs-windows-test`
- **Depends on:** #6
- **Goal:** production-grade platform glue.
- **Files:** `src-tauri/src/platform/windows.rs`, `src-tauri/src/services/schedule.rs`.
- **Requirements:** UAC elevation for machine-scope installs; toast notifications; background
  scheduled update scan via Task Scheduler; offline MSIX install from file; winget
  import/export.
- **Acceptance:** elevation path works; scheduled scan produces a toast; import replays a
  manifest list.
- **LLM note:** `tier:max` (Windows platform APIs).