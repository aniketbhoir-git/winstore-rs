# Open-Source Windows App Storefront — LLM Implementation Specification

> **Purpose:** this document is a hand-off spec for a coding LLM (e.g. Claude Sonnet 4.5 on
> OpenRouter). It contains **no code** — only precise, step-by-step instructions about how the
> application works and how the code should be generated, structured, and verified.
>
> **It is written to be safe to paste into any model:** it describes a legitimate, independent,
> open-source application manager and storefront for Windows built on public, documented, and
> community-maintained interfaces (winget, MSIX, package deployment APIs). It deliberately uses
> positive, neutral terminology and asks for nothing beyond the stated scope. Instruct the
> coding LLM to implement exactly against the interfaces named here.

---

## 0. Terminology & ground rules (give these to the coding LLM)

Use these terms only:

Use this terminology and framing throughout, keeping every description positive and within
public, documented interfaces:

- winget (Windows Package Manager) CLI and its sources
- winget community manifest repository (`microsoft/winget-pkgs`)
- winget `msstore` source — the Store catalog REST service that winget itself uses
- MSIX / AppX package deployment via the Windows Deployment API
- Public catalog metadata (name, version, publisher, screenshots, ratings)
- "Buy in Microsoft Store" browser hand-off (https://apps.microsoft.com/detail/{productId})
- winget community REST catalogs (`winget-api.dev`, `winget.run`)

Out of scope, never to be implemented or researched further: payment, account sign-in, and
any licensing/entitlement or content-protection system. When a paid item appears, the only
supported action is the official browser hand-off.

Hard rules for the generated code:

1. No payment, account sign-in, or licensing/entitlement handling of any kind. Any item flagged
   `is_paid=true` shows only a call-to-action that opens the official product page in the
   user's default browser (https://apps.microsoft.com/detail/{productId} or
   ms-windows-store://pdp/?ProductId={productId}).
2. Catalog sources only ever read public, machine-readable data over HTTPS. Never touch or
   attempt to authenticate against or scrape account/proprietary service internals.
3. Product name/branding must be original; do not reference Microsoft Store trademarks, logos,
   or artwork in the shipped app.
4. Every destructive-or-silent installer action must originate from an explicit user click and
   respect winget's own source/package agreements (surface them; never pre-accept silently).

---

## 1. System overview

A desktop application for Windows that finds, installs, updates, pins, and removes applications,
backed by a pluggable set of *sources* (catalogs) and *executors* (installs/removals).
Eventually it also exposes a small local web UI for managing headless Windows Server machines.

### 1.1 Layers

1. **Frontend UI** — a web frontend running inside Tauri 2's WebView2 shell, styled to look like
   a modern app store (cards, app detail pages, progress).
2. **IPC boundary** — typed Tauri commands; the frontend never talks to winget or the network
   directly; all state changes flow through the Rust core and are pushed back as events.
3. **Rust core** — domain model, sources, executors, services, persistence, and the optional
   local web server. Single crate under `src-tauri/`.
4. **Platform glue** — Windows elevation, toasts, scheduled update scan, winget discovery.
5. **Persistence** — SQLite for cache, install state, settings, and an operation log.

### 1.2 Concurrency & async model

- Tokio runtime (Tauri provides one). All network and subprocess work is async.
- Long operations (install/update) run as cancellable tasks; each emits a stream of
  `ProgressEvent` records that the IPC layer relays to the frontend as events.
- At most one mutating operation per package id at a time; a global `OpGuard` (per-package
  `Mutex` map) prevents races.

### 1.3 Error model

- One crate-wide error enum with variants: `Io`, `Sql`, `Http`, `WingetNotFound`,
  `WingetFailed(code, stderr)`, `SourceUnavailable`, `PackageNotFound`, `DeploymentFailed`,
  `Cancelled`, `Unsupported`, `ElevationRequired`, `InvalidConfig`.
- Every error maps to: (stable string `code`, human message, optional raw detail for logs,
  is_retryable flag). IPC returns this as JSON; logs record the raw detail.

---

## 2. Locked technology decisions

| Concern | Decision |
|---|---|
| UI shell | Tauri 2 + WebView2 |
| Frontend framework | Svelte (or React if preferred) + TypeScript; plain CSS with a light/dark Fluent-like theme |
| Backend language | Rust (stable) |
| Async | tokio |
| HTTP | reqwest + serde/serde_json |
| Persistence | SQLite via sqlx (async) with embedded migrations |
| Windows APIs | official `windows` crate (Deployment `PackageManager`, notifications, AppModel) |
| Install/remove engine (primary) | winget CLI invoked as a subprocess for install/update/remove |
| Install/remove engine (secondary) | `Windows.Management.Deployment.PackageManager` for direct MSIX add/remove |
| Optional sources | Scoop, Chocolatey (their CLIs as subprocess executors) |
| Headless mode | axum web server exposing the same core services as a localhost HTTP/JSON API |
| Logging | tracing; operation log mirrored into SQLite |

Rationale to record for the LLM: winget already solves installer execution, hash/agreement
handling, dependencies, and MSIX deployment correctly; wrapping it is far safer than
reimplementing installer logic. The catalog layer is kept separate (pure HTTP) so search is
fast and typed rather than scraping winget's human-readable output.

---

## 3. Repository layout (the LLM must create these paths)

```
src-tauri/
  Cargo.toml
  tauri.conf.json
  src/
    main.rs            # entry: Tauri builder, plugin registration, logging init
    lib.rs             # module re-exports (Tauri mobile convention)
    error.rs           # unified error enum + display + serialization
    config.rs          # user config load/save, defaults
    domain/
      package.rs       # PackageSummary, PackageDetail, SourceKind, PackagingKind
      source.rs        # CatalogSource trait + SourceCapabilities
      install.rs       # InstallState, PackageVersion, UpdateAvailable
      progress.rs      # ProgressEvent (op, package_id, stage, fraction, message)
    db/
      mod.rs           # pool init
      migrations/      # ordered SQL migrations
      models.rs        # row <-> domain mapping
    catalog/
      mod.rs           # coordinator: fan-out search across enabled sources, merge/dedupe
      search.rs        # query model (text, category, source, sort, page)
    sources/
      winget_repo.rs   # community manifest catalog via REST
      msstore.rs       # winget msstore source (Store catalog) — best-effort
      scoop.rs         # Scoop buckets
      choco.rs         # Chocolatey community feed
    executor/
      mod.rs           # PackageExecutor trait + registry
      winget_cli.rs    # winget subprocess wrapper + output parser + progress
      package_manager.rs# direct MSIX deployment via windows crate
      scoop_cli.rs
      choco_cli.rs
    services/
      search.rs        # search orchestration + cache
      install.rs       # install/remove/cancel lifecycle
      upgrade.rs       # update scan + update one/all
      library.rs       # installed app list + local state
      schedule.rs      # scheduled update scan (background)
    ipc/
      commands.rs      # every #[tauri::command] handler
      events.rs        # event names + payload types
      state.rs         # AppState dependency graph
    platform/
      windows.rs       # elevation, toast, winget discovery, scheduled task
    server/
      mod.rs           # feature-gated axum server
      api.rs           # HTTP/JSON routes over the same core
      state.rs         # shared core state for the web UI
ui/
  src/
    lib/               # typed API client for Tauri commands + event subscription
    components/        # AppCard, DetailHeader, InstallButton, ProgressBar, SourceBadge,
                       # RatingStars, UpdateRow, BuyInStoreButton, SourcePicker
    routes/            # Search, AppDetail, Library, Updates, Settings, Remote
  index.html
docs/
  adr/                 # short decision records
  api-notes/           # notes on each catalog endpoint shape (for humans)
```

---

## 4. Domain & data model (describe; the LLM turns this into structs + SQL)

### 4.1 Enumerations
- `SourceKind`: WingetCommunity, MsStore, Scoop, Chocolatey.
- `PackagingKind`: Msix, MsixBundle, Exe, Msi, Portable, Unknown.
- `InstallState`: NotInstalled, Installing, Installed, UpdateAvailable, Uninstalling, Failed, Pinned.
- `OperationStage`: queued, resolving, downloading, installing, finalizing, done, failed, cancelled.

### 4.2 Core structs (fields only — LLM writes the Rust)
- `PackageSummary`: id (stable, e.g. `src/pkg`), name, publisher, latest version, one-line
  description, icon url, rating, rating count, source kind, is_paid, packaging kind.
- `PackageDetail`: everything in Summary plus full description, screenshots, website, license,
  release notes, installer metadata (architecture, install type, size, sha256), map of
  available versions.
- `ProgressEvent`: operation id, package id, operation, stage, fraction (0.0–1.0), message,
  timestamp.
- `UpdateAvailable`: package id, installed version, latest version, source, is_auto.

### 4.3 SQLite schema (tables)
- `packages` — cached catalog rows; key `(source, package_id)`; `fetched_at` for TTL.
- `installs` — local install ledger: package id, state, installed_version, target_version,
  pinned, auto_update, last_update_check, last_updated.
- `sources` — enabled/disabled, priority, kind, extra config, last_sync.
- `settings` — key/value: theme, cache_ttl, update interval, default source exclusions.
- `operation_log` — id, ts, level, operation, package_id, message, detail (for UI "log" pane).

Rules: SQLite used only for cache + local state (never auth); all DB access behind async
repository functions; migrations are additive and versioned.

---

## 5. Service contracts (traits — LLM writes signatures, not bodies here)

### 5.1 `CatalogSource`
Methods (exact names the LLM should use): `search(query)`, `get_detail(package_id)`,
`get_featured(limit)`, `list_categories()`, `capabilities()`.
Every method returns our domain types and an `AppError`. Sources declare a `SourceKind` and a
human display name. Rate limits and timeouts live inside each adapter.

### 5.2 `PackageExecutor`
Methods: `install(package, request) -> stream of ProgressEvent`, `cancel(operation_id)`,
`remove(package) -> stream of ProgressEvent`, `list_installed()`, `list_updates()`,
`update(package) -> stream`, `pin(package)`, `unpin(package)`, `is_available()`.
Executors are registered per source and selected by `PackagingKind`/capability.

### 5.3 Coordinator
- Search fans out to all enabled `CatalogSource`s concurrently (bounded concurrency), merges and
  dedupes by normalized name+publisher, sorts, applies cache TTL, paginates.
- Install/update delegates to the executor implied by the package's source/packaging kind.

---

## 6. Data-flow walkthroughs (LLM must implement exactly these)

### 6.1 Search
1. Frontend calls `search_sources(query, filters)`.
2. Core checks per-source cache; for each stale enabled source, issues async catalog request.
3. Merge results -> `Vec<PackageSummary>` -> return; persist fetched rows to `packages`.
4. Frontend renders cards; clicking one calls `get_app_detail`.

### 6.2 Install (community app)
1. `start_install(package_id)` creates an operation, sets `InstallState::Installing`, opens a
   `ProgressEvent` stream.
2. Executor resolves the package to a concrete installer via `winget install --id ... --accept-*
   --scope user/machine --silent --verbose-logs` (agreements surfaced, never auto-accepted
   without the user's prior consent in settings).
3. winget's stdout/stderr is parsed into staged progress; each parse updates the stream + DB.
4. On exit code 0: mark `Installed`, record in `installs`. Non-zero: `Failed` with stderr detail;
   stream ends with a terminal event.
5. Frontend shows progress bar + a log tail; `cancel_install` sends a cancel token.

### 6.3 Update all
1. `check_updates()` runs `winget upgrade --all --include-unknown` in dry-run/list mode (or
   `winget list --upgrade-available`) plus installed-package query from `PackageManager`.
2. Returns `Vec<UpdateAvailable>`; frontend shows list + "Update all".
3. `update_all()` runs updates sequentially (configurable concurrency = 1), emitting progress per
   package; results aggregated into a summary event.

### 6.4 Purchase hand-off (never in-app)
1. A detail row with `is_paid=true` renders a **Buy in Microsoft Store** button instead of
   Install.
2. Core's `open_store_page(product_id)` calls the OS to open the official product URL in the
   default browser. Nothing else happens in-app.

### 6.5 Headless / remote (Phase 3)
1. `start_server(port)` boots the optional axum server bound to localhost (or LAN with explicit
   user opt-in + token).
2. It exposes the same operations as JSON endpoints: search, detail, install, cancel, update,
   library, log stream (SSE), using the identical core services (no separate logic).
3. A minimal web UI (same components, no Tauri layer) talks to it for server boxes.

---

## 7. Work packages (generate in this order)

**WP0 — Scaffold & CI.** Create crate, Tauri 2 bootstrap, `error.rs`, `config.rs`, SQLite pool +
migrations, tracing. Add GitHub Actions: `windows-latest` build + `cargo test` + clippy + fmt.
*Accept:* a window opens, DB initializes, config round-trips.

**WP1 — Domain + DB.** Implement §4 enums/structs, migrations, repositories.
*Accept:* unit tests pass for row<->domain mapping and migration apply-from-empty.

**WP2 — Catalog source: community.** Implement `CatalogSource` trait + `winget_repo.rs` using a
public JSON catalog endpoint; parse into `PackageSummary`. Mock the endpoint in tests
(httpmock).
*Accept:* `search("firefox")` returns typed summaries in an integration test without winget.

**WP3 — Executor: winget CLI.** Implement `winget_cli.rs`: discover winget path, version check,
install/remove/update wrappers, stdout parser (progress + errors), cancel via process kill +
winget's own cancel. Fake `winget` executable fixture for tests.
*Accept:* tests drive install/update/remove against the fake and parse every stage; a manual
smoke test installs one small package on a real Windows box.

**WP4 — IPC + UI skeleton.** Define all Tauri commands/events; create frontend with search page
(debounced input -> results grid) and install button wiring.
*Accept:* search from UI shows real community results; clicking Install starts a real install
with live progress.

**WP5 — Detail + full install loop (MVP).** Detail page (screenshots, description, versions,
rating), cancel, uninstall, log tail. *Accept:* end-to-end install/uninstall/cancel of a
community app from the UI is reliable. **This is the MVP milestone.**

**WP6 — Library + updates.** Installed list, update badge, update one/all with summary.
*Accept:* update scan finds an outdated throwaway package; update-all converges, pins hold.

**WP7 — msstore source + purchase hand-off.** `msstore.rs` adapter; `is_paid` detection;
Buy-in-Store button. *Accept:* free Store apps are resolvable; a paid item shows only the
browser hand-off (no install path).

**WP8 — Polish.** Categories/featured, ratings/screenshots caching, dark/light theme, empty/
error/loading states, accessibility pass.
*Accept:* visual parity checklist passes; keyboard navigable.

**WP9 — Extra sources.** Scoop + Chocolatey adapters behind the same traits; source picker in UI.
*Accept:* a Scoop-only app and a Chocolatey-only app each install from their source.

**WP10 — Headless mode.** `server/` axum API + minimal web UI; feature flag `server`.
*Accept:* manage a Windows Server 2022 box from a second machine's browser (install/list/update).

**WP11 — Hardening.** UAC elevation for machine-scope, toast notifications, background scheduled
update scan (Task Scheduler), offline MSIX install from file, winget import/export.
*Accept:* elevation path works for admin-scope installs; scheduled scan produces toasts; import
replays a manifest list.

---

## 8. Testing & QA strategy

- **Unit:** parsers (winget output, catalog JSON), state transitions, dedupe/sort, config.
- **Integration (HTTP):** `httpmock`/`wiremock` serving canned catalog JSON; assert parsing and
  error handling for malformed/rate-limited responses.
- **Subprocess:** a `fake winget` fixture (a script/binary printing scripted stdout and exit
  codes) exercises the executor and progress parser deterministically.
- **E2E (Windows CI):** on `windows-latest`, install→verify→uninstall a tiny open-source package;
  on a Windows Server 2022 runner, run the axum server and drive it remotely.
- **Manual checklist:** elevation prompt, cancellation mid-download, offline behavior, telemetry
  is fully off (no phoning home beyond the catalogs the user enabled).

---

## 9. Guardrail & review checklist (run before every merge)

1. `is_paid` items have no install path in-app; only the browser hand-off.
2. No auth/account/secret handling introduced anywhere.
3. Sources hit only the HTTPS endpoints declared here; timeouts + retry backoff set; no
   guessing/scanning endpoints.
4. Agreements/source terms surfaced to the user; never silently pre-accepted.
5. No Microsoft trademark/logo assets bundled; original product identity.
6. Searchable log of every install/uninstall/update with timestamp and exit status.