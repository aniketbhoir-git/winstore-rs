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

> **Status of this document (2026-11-11):** for work packages WP0 to WP5 this document is the
> contract for the code that exists. For WP6 to WP11 it is the plan. §7 gives the status of each
> work package. Where the text and the code of a built work package disagree, the code is the
> fact: stop, report the difference, and correct this document. Do not rebuild code that exists.
> winget flags, exit codes and output formats are in the `winget-executor` skill
> (`.claude/skills/winget-executor/SKILL.md`), not here. Open review actions are in
> `docs/reviews/2026-11-11-spec-review.md`.

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
  `Cancelled`, `Unsupported`, `ElevationRequired`, `InvalidConfig`,
  `AgreementsRequired { source_agreements, package_agreements }` (code `agreements_required`;
  see §6.6).
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

## 3. Repository layout (target)

The tree shows the target layout. A path exists only when its work package is built. The layout
as built is in `CLAUDE.md`, "Layout (as built)". Differences on 2026-11-11:

- Not created yet (the work package that creates each one): `services/upgrade.rs`,
  `services/library.rs` (WP6); `sources/msstore.rs` (WP7); `sources/scoop.rs`,
  `sources/choco.rs`, `executor/scoop_cli.rs`, `executor/choco_cli.rs` (WP9); `server/` (WP10);
  `platform/`, `services/schedule.rs`, `executor/package_manager.rs` (WP11).
- `services/search.rs` is not planned. Search lives in `catalog/mod.rs` and `catalog/search.rs`.
- The update scan and update-all live in `ipc/commands.rs` (`run_check_updates`,
  `run_update_all`) until WP6 moves them to `services/upgrade.rs`.
- Built and not in the tree: `executor/winget_parse.rs` (progress parser, exit-code map),
  `db/repo/*.rs` (repositories), `ipc/test_support.rs` (shared test fakes).
- `ui/src/routes/` holds `Search` and `AppDetail` only.

```text
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
Only `install`, `remove`, `update`, `pin` and `unpin` may change the system. `list_installed`,
`list_updates` and `is_available` are read-only: they must not install, update or remove
anything.

### 5.3 Coordinator
- Search fans out to all enabled `CatalogSource`s concurrently (bounded concurrency), merges and
  dedupes by normalized name+publisher, sorts, applies cache TTL, paginates.
- Install/update delegates to the executor implied by the package's source/packaging kind.

### 5.4 IPC commands and events (as built, WP4 and WP5)

All handlers are in `ipc/commands.rs`. Each one wraps a plain async function that the tests in
`ipc/tests.rs` call. A new command needs a row here in the same pull request.

| Command | Arguments | Returns | Behavior |
|---|---|---|---|
| `app_version` | none | string | The crate version. |
| `search_sources` | `search_id`, `query` | `Vec<PackageSummary>` | Catalog search. Also emits `store:search-results` or `store:search-failed` with the same `search_id`. |
| `get_app_detail` | `package_id` | `PackageDetail` | Catalog detail. |
| `start_install` | `package_id`, `options?` (`version?`, `scope?`) | `OperationStarted { operation_id }` | Refuses a paid item. Checks the ledger, starts winget, sets `Installing`. |
| `start_uninstall` | `package_id` | `OperationStarted` | Needs a ledger row. Sets `Uninstalling`. |
| `cancel_install` | `operation_id` | nothing | Cancels an install, an uninstall or an update. An unknown or finished id is not an error. |
| `get_install_status` | `package_id` | `InstallStatus` | The ledger row and the active operation, if one runs. No row gives `NotInstalled`. |
| `get_operation_log` | `package_id`, `limit?` | `Vec<OperationLogEntry>` | The log tail, oldest first. Default 100 rows, maximum 500. |
| `check_updates` | none | `Vec<UpdateAvailable>` | Read-only list (§6.3). Does not write the ledger. |
| `update_all` | none | `UpdatesSummaryEvent` | Sequential updates with progress. Stays in WP6 scope; no UI uses it yet. |
| `open_store_page` | `product_id` | nothing | Checks the id, then opens the official product page in the browser. |
| `start_server` | `port` | error `unsupported` | Stub until WP10. |

Not built: pin and unpin commands, a library list, settings read and write, source enable and
disable. The commands for agreement consent (§6.6) are also not built.

| Event | Payload |
|---|---|
| `store:progress` | `ProgressEvent` |
| `store:search-results` | `search_id`, `results` |
| `store:search-failed` | `search_id`, `error` |
| `store:operation-finished` | `operation_id`, `package_id`, `ok`, `error?` |
| `store:updates-summary` | `total`, `succeeded`, `failed`, `outcomes` |

For each operation, the last `store:progress` event carries the terminal stage. Then the core
settles the ledger. Then it emits `store:operation-finished`. A cancel gives `ok = false` with
the error code `cancelled`; the UI must not show that as a failure.

---

## 6. Data-flow walkthroughs (LLM must implement exactly these)

### 6.1 Search
1. Frontend calls `search_sources(query, filters)`.
2. Core checks per-source cache; for each stale enabled source, issues async catalog request.
3. Merge results -> `Vec<PackageSummary>` -> return; persist fetched rows to `packages`.
4. Frontend renders cards; clicking one calls `get_app_detail`.

### 6.2 Install (community app)
1. `start_install(package_id)` reads the detail, refuses a paid item, checks the ledger (§6.7),
   starts winget, sets `InstallState::Installing` and opens a `ProgressEvent` stream.
2. The executor runs a silent, non-interactive winget install with an exact id match and a
   named source. It passes an agreement flag only for consent that the user gave (§6.6). The
   flags are in the `winget-executor` skill.
3. winget's stdout/stderr is parsed into staged progress; each parse updates the stream.
4. The exit code decides the result, and §6.7 gives the ledger state for each result. The
   stream ends with one terminal event.
5. Frontend shows progress bar + a log tail; `cancel_install` stops the winget process tree
   (§6.7).

### 6.3 Update all
1. `check_updates()` lists the available updates and changes nothing on the system. It runs
   `winget upgrade` with no package argument and no `--all`; that form only prints the list.
   Never use `--all` in a check: `winget upgrade --all` installs every update.
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

### 6.6 Agreement consent (WP5; not built)

Hard rule 4 applies. The app never accepts an agreement that the user did not see. There is no
global setting that accepts agreements.

Two kinds of agreement exist, and each has its own rule:

| Kind | Consent | Storage |
|---|---|---|
| Source agreement | Once per source. | `sources` table: the time of consent for that source. |
| Package agreement | Once per operation. The user sees it each time. | None. The consent lives for one operation only. |

Flow:

1. An operation starts with no package agreement flag. It passes the source agreement flag
   only when the `sources` table holds consent for that source.
2. When winget needs an agreement, the operation ends with the error `agreements_required`.
   The two fields of the error say which kind is missing. The ledger goes back to its stable
   state (§6.7, "failed" rows). This result is not a failure in the UI.
3. The UI opens a consent dialog. The dialog names the package and the source, says which
   kind of agreement is necessary, and shows the agreement text that winget gives.
4. **Accept.** For a source agreement the UI calls `accept_source_agreement(source)`, which
   stores the consent. For a package agreement the UI sets
   `options.accept_package_agreements = true`. Then the UI calls the start command again.
   When the two kinds are necessary, winget reports them one at a time, and the dialog can
   open two times.
5. **Decline.** Nothing runs. The ledger does not change. The core writes one Info row to
   `operation_log`: "agreements declined".
6. The user can remove a source consent in the source settings (WP6 or later; until then the
   consent stays).

Changes that WP5 owns:

- Remove `accept_source_agreements` from `AppConfig`. Today it sets the two flags for each
  operation, which is the silent acceptance that rule 4 forbids. It defaults to false, and no
  screen sets it, so no user loses a setting.
- Add the consent column to `sources` with an additive migration.
- Add the `accept_source_agreement` command and the `accept_package_agreements` option on
  `start_install`. `update_all` (WP6) stops at a package that needs an agreement, reports it in
  the summary, and continues with the next package.
- Add the consent dialog. The UI must read `error.code`; today it reads only `cancelled`.
- Tests: the fake winget returns each of the two agreement exit codes; an accepted retry
  passes the correct flag; a decline leaves the ledger as it was.

Open point for the implementer: find how to read the agreement text with no interactive
prompt, and record the method in the `winget-executor` skill. If winget gives no text for a
case, stop and report. Do not show a dialog that has no text.

### 6.7 Install state transitions (WP5, as built)

Terms. The *ledger* is the `installs` row. An operation is *active* while its winget process
runs in this app process. The *stable state* is the state the ledger had before the operation
started: `Installed` when a version is recorded, `NotInstalled` or `Failed` when not.

`Installing` and `Uninstalling` are the only busy states; an update uses `Installing`. The legal
edges are in `domain/install.rs`. An illegal edge returns the error `unsupported` and changes
nothing.

**Start**

| Ledger state | Command | Result |
|---|---|---|
| No row, `NotInstalled`, `Installed`, `UpdateAvailable`, `Failed` | `start_install` | `Installing`; `target_version` set. |
| `Installing` with no active operation (a leftover) | `start_install` | `Installing`. |
| `Uninstalling`, `Pinned` | `start_install` | No change; `unsupported`. |
| Any, paid item | `start_install` | No change; `unsupported`. |
| `Installed`, `UpdateAvailable`, `Failed`, `Pinned` | `start_uninstall` | `Uninstalling`. |
| No row, `NotInstalled`, `Installing`, `Uninstalling` | `start_uninstall` | No change; `unsupported`. |
| Any, an operation is active for the package | Either | No change; `unsupported`. The message names the running operation. |
| Any, winget is absent | Either | No change; `winget_not_found`. |

A second click during an operation and an uninstall during an install are the "operation is
active" row. The UI also disables the buttons while an operation runs.

**End of an operation**

`target_version` is cleared in each row. The executor writes the `operation_log` row.

| Operation | Outcome | Terminal stage | Ledger state | Log row | Error code |
|---|---|---|---|---|---|
| Install, update | Success | `done` | `Installed`; version = target | Info | none |
| Install, update | Failure, a version was installed before | `failed` | `Installed`; version kept | Error | from the exit code |
| Install | Failure, no version before | `failed` | `Failed` | Error | from the exit code |
| Install, update | Cancel, a version was installed before | `cancelled` | `Installed`; version kept | Info | `cancelled` |
| Install | Cancel, no version before | `cancelled` | `NotInstalled` | Info | `cancelled` |
| Uninstall | Success | `done` | `NotInstalled`; version and pin cleared | Info | none |
| Uninstall | Failure | `failed` | `Failed`; version kept | Error | from the exit code |
| Uninstall | Cancel | `cancelled` | `Installed` | Info | `cancelled` |

A row that has the pin flag goes back to `Pinned` where the table says `Installed`. The exit
code map is in the `winget-executor` skill.

**Cancel**

- Cancel does the same thing in each stage: it stops the winget process tree. The app uses a
  Job Object first and `taskkill /T /F` second. winget has no cancel call.
- When the process ended before the cancel arrived, the real outcome stands. An exit code of 0
  after a cancel is a success.
- Cleanup after a cancel means: the process tree is stopped, `target_version` is cleared, and
  the ledger is in its stable state. The row stays. The app does not delete downloaded files
  and does not undo an installer that already started to write; winget and the installer own
  those files.
- When the app cannot stop the tree (an elevated installer), the operation continues and its
  real outcome settles the ledger. The executor writes a Warn log row. A second cancel for
  the same operation does nothing.

**Restart**

On the first use of the core after a start, each row in `Installing` becomes `Installed` when a
version is recorded and `Failed` when not. Each row in `Uninstalling` becomes `Failed`. The core
writes one Warn log row per row. It does not look for a winget process that survived the last
run.

**Known gaps**

| Gap | Owner |
|---|---|
| A cancel that cannot stop the tree is not reported to the UI. The Cancel button stays and does nothing. Emit a `store:progress` event with a message, and disable the button. | WP5 |
| Agreement consent (§6.6). | WP5 |
| Cancel that depends on the stage, and cancel of an elevated installer (review A12). | WP11 |
| The exit code is not stored in `operation_log`; the "nothing to do" exit code counts as success for each operation, uninstall also (review A13). | WP6 |
| Nothing writes `Pinned` or `UpdateAvailable`; no pin command exists (review A10). | WP6 |
| No check of the ledger against the installed list after a restart or a failed uninstall (review A10). | WP6 |
| A service-layer failure to settle the ledger goes to the trace log only. | WP6 |
| The UI never sets the install scope, so winget uses its default (review A24). | WP11 |

---

## 7. Work packages (generate in this order)

Status on 2026-11-11. The issue numbers are the GitHub issues and the entries of
`docs/ISSUE-BACKLOG.md`.

| WP | Issue | Status |
|---|---|---|
| WP0 | #1 | Built, on `main`. |
| WP1 | #2 | Built, on `main`. |
| WP2 | #3 | Built, on `main` (PR #14). The catalog service returns unrelated results: bug F1, #20. |
| WP3 | #4 | Built, on `main` (PR #15). |
| WP4 | #5 | Built, on `main` (PR #18). |
| WP5 | #6 | Built on branch `impl/6--install-loop` (PR #19, open). Not merged. To close: §6.6, the WP5 rows of the gap table in §6.7, and F1. |
| WP6 to WP11 | #7 to #12 | Not started. `check_updates` and `update_all` exist as commands with no UI. |

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
install/remove/update wrappers, stdout parser (progress + errors), cancel by a stop of the
process tree (Job Object, then `taskkill /T /F`); winget has no cancel call. Fake `winget`
executable (the cargo example `fake_winget`) for tests.
*Accept:* tests drive install/update/remove against the fake and parse every stage; a manual
smoke test installs one small package on a real Windows box.

**WP4 — IPC + UI skeleton.** Define all Tauri commands/events; create frontend with search page
(debounced input -> results grid) and install button wiring.
*Accept:* search from UI shows real community results; clicking Install starts a real install
with live progress.

**WP5 — Detail + full install loop (MVP).** Detail page (screenshots, description, versions,
rating), cancel, uninstall, log tail, agreement consent (§6.6). Commands: `start_uninstall`,
`get_install_status`, `get_operation_log` (§5.4). State rules: §6.7.
*Accept:* each row of the two transition tables in §6.7 has a test that passes against the fake
winget; the consent tests of §6.6 pass; a cancelled install leaves the ledger in its stable
state with `target_version` cleared. **This is the MVP milestone.**

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