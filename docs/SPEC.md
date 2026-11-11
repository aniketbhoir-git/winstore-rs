# Store — implementation specification

> **Objective:** Store is an open-source desktop app for Windows that finds, installs, updates
> and removes applications through the Windows Package Manager (winget). This document defines
> how the app works: its rules, its data, its commands and its work packages. It contains no
> code.

> **Status of this document (2026-11-11):** WP0 to WP4 are on `main`. WP5 is on branch
> `impl/6--install-loop` (PR #19, open). WP6 to WP11 are not started. §7 gives the status of
> each work package.
>
> - A section marked *built* is the contract for code that exists. Where it and the code
>   disagree, the code is the fact: stop, report the difference, and correct this document. Do
>   not rebuild code that exists.
> - A section marked *planned* is the plan. It overrides the code.
> - winget flags, exit codes and output formats are in the `winget-executor` skill
>   (`.claude/skills/winget-executor/SKILL.md`), not here. **Exception:** for agreement flags,
>   §6.6 overrides the skill until WP5 updates the skill.
> - Open review actions (ids A1 to A25) are in `docs/reviews/2026-11-11-spec-review.md`.

---

## 0. Terminology & ground rules

Use these terms:

- winget (Windows Package Manager) CLI and its sources
- winget community manifest repository (`microsoft/winget-pkgs`)
- winget `msstore` source, reached only through the winget CLI. It is off by default; the user
  turns it on. On failure it returns `SourceUnavailable`.
- MSIX / AppX package deployment via the Windows Deployment API
- Public catalog metadata (name, version, publisher, screenshots, ratings)
- "Buy in Microsoft Store" browser hand-off (`https://apps.microsoft.com/detail/{productId}`)
- The community catalog backend named in `docs/adr/0001-community-catalog-backend.md`. Until
  that record is accepted, the code reads `https://api.winget.run`, which returns stale rows
  (bug F1 in `docs/ISSUE-BACKLOG.md`).

Out of scope, never to be implemented or researched further: payment, account sign-in, and
any licensing/entitlement or content-protection system. When a paid item appears, the only
supported action is the official browser hand-off.

Hard rules:

1. No payment, account sign-in, or licensing/entitlement handling of any kind. Any item flagged
   `is_paid=true` shows only a call-to-action that opens the official product page in the
   user's default browser (`https://apps.microsoft.com/detail/{productId}` or
   `ms-windows-store://pdp/?ProductId={productId}`).
2. Catalog sources only ever read public, machine-readable data over HTTPS. Never touch or
   attempt to authenticate against or scrape account/proprietary service internals.
3. The product name and branding are original. Do not use Microsoft logos or artwork. The text
   label "Buy in Microsoft Store" on the hand-off button is permitted.
4. Every destructive-or-silent installer action must originate from an explicit user click and
   respect winget's own source/package agreements (surface them; never pre-accept silently).
   §6.6 defines the consent flow.
5. No telemetry. The app makes network calls only to the catalogs that the user turned on, and
   through winget.

---

## 1. System overview

A desktop application for Windows that finds, installs, updates, pins, and removes applications,
backed by a pluggable set of *sources* (catalogs) and *executors* (installs/removals).
WP10 adds a mode for headless Windows Server machines; its design is not decided (§6.5).

### 1.1 Layers

1. **Frontend UI** — a web frontend running inside Tauri 2's WebView2 shell, styled to look like
   a modern app store (cards, app detail pages, progress).
2. **IPC boundary** — typed Tauri commands; the frontend never talks to winget or the network
   directly; all state changes flow through the Rust core and are pushed back as events.
3. **Rust core** — domain model, sources, executors, services and persistence. Single crate
   under `src-tauri/`.
4. **Platform glue** — Windows elevation, toasts, scheduled update scan, winget discovery.
5. **Persistence** — SQLite for cache, install state, settings, and an operation log.

### 1.2 Concurrency & async model

- Tokio runtime (Tauri provides one). All network and subprocess work is async.
- Long operations (install/update) run as cancellable tasks; each emits a stream of
  `ProgressEvent` records that the IPC layer relays to the frontend as events.
- At most one mutating operation per package id at a time; a global `OpGuard` (per-package
  `Mutex` map) prevents two operations on the same package id.
- The lock is per package and lives in one process. It does not stop two installs of
  different packages, and it does not cover a second app instance. §6.7 lists the two gaps.

### 1.3 Error model

- `AppError` is the one crate-wide error enum. Each variant has a stable string code:

  | Variant | Code | Retryable |
  |---|---|---|
  | `Io` | `io` | Yes |
  | `Sql` | `sql` | Yes |
  | `Http` | `http` | Yes |
  | `WingetNotFound` | `winget_not_found` | No |
  | `WingetFailed { code, stderr }` | `winget_failed` | Yes |
  | `SourceUnavailable` | `source_unavailable` | Yes |
  | `PackageNotFound` | `package_not_found` | No |
  | `DeploymentFailed` | `deployment_failed` | No |
  | `Cancelled` | `cancelled` | No |
  | `Unsupported` | `unsupported` | No |
  | `ElevationRequired` | `elevation_required` | No |
  | `InvalidConfig` | `invalid_config` | No |
  | `AgreementsRequired { source_agreements, package_agreements }` | `agreements_required` | No |

- Every error maps to: (stable string `code`, human message, optional raw detail for logs,
  is_retryable flag). IPC returns this as JSON; logs record the raw detail.
- In one `AgreementsRequired` error, exactly one of the two fields is true. winget reports one
  kind of agreement at a time.

### 1.4 Goals and non-goals

Goals, in the order of the work packages:

1. Search a community catalog and install, uninstall and cancel from the UI (WP0 to WP5, the
   MVP).
2. Show installed apps and update them (WP6).
3. Show Microsoft Store items; a paid item gets the browser hand-off only (WP7).
4. More sources, a headless mode and platform integration (WP8 to WP11).

The reasons and the users are in `docs/STRATEGY.md` §1.2 (feature triage) and §1.3 (use cases).

Non-goals:

- Payment, account sign-in, licensing. See hard rule 1.
- Installer logic of our own. winget runs each installer.
- Telemetry. See hard rule 5.
- A catalog service that the project hosts. ADR 0001 keeps this as a later option.

---

## 2. Technology decisions

| Concern | Decision |
|---|---|
| UI shell | Tauri 2 + WebView2 |
| Frontend framework | Svelte + TypeScript; plain CSS with a light/dark Fluent-like theme |
| Backend language | Rust (stable) |
| Async | tokio |
| HTTP | reqwest + serde/serde_json |
| Persistence | SQLite via sqlx (async) with embedded migrations |
| Windows APIs | official `windows` crate (notifications, AppModel) |
| Install/remove engine | winget CLI invoked as a subprocess for install/update/remove |
| Logging | tracing; operation log mirrored into SQLite |

Rationale: winget handles installer execution, hash checks, agreements, dependencies and MSIX
deployment. The app wraps it and does not write installer logic.

### 2.1 Deferred, needs a decision

These items are not decided. Do not build them and do not add their dependencies.

| Item | Work package | Waits for |
|---|---|---|
| Headless mode and its web server (axum was the first idea) | WP10 | A decision record (review action A5) |
| A second executor: `Windows.Management.Deployment.PackageManager` for a local MSIX file | WP11 | Review action A9. v1 has one executor, chosen by `SourceKind`. |
| Scoop and Chocolatey sources and executors | WP9 | WP9 start |

### 2.2 Alternatives considered

| Decision | Options | Status |
|---|---|---|
| UI shell | Tauri 2 against other Rust UI toolkits | Decided. See `docs/STRATEGY.md` §2.3. |
| Community catalog backend | (a) the `winget.run` REST mirror, in the code today; (b) `winget search` and `winget show` through the executor; (c) the official winget index file; (d) the winget COM API; (e) a catalog that the project builds | Proposed: (b). See `docs/adr/0001-community-catalog-backend.md`. (a) returns stale rows. (c) has no documented format. |
| Executor interface | (a) the winget CLI, built; (b) the winget COM API `Microsoft.Management.Deployment` | (a) is built and stays. (b) gives typed progress and a cancel call, and removes text parsing. It needs generated bindings, and access from an app with no package identity is not verified. Open; decide before WP6. |
| Headless shape | A local web server, a command-line tool, or remote desktop only | Open (review action A5). |

### 2.3 Risks and mitigations

`docs/STRATEGY.md` §5 holds the project risks. This table holds the risks that this design
adds.

| Risk | Mitigation or owner |
|---|---|
| The app accepts an agreement that the user did not see. | §6.6: the app stores no consent and passes a flag only directly after a user "Accept". |
| winget gives no agreement text without an interactive prompt, so the consent dialog has nothing to show. | The spike in §6.6 comes first in WP5. A fallback is named there and needs maintainer approval. |
| The third-party catalog is stale or wrong (bug F1). | ADR 0001. Until it is accepted, the search results are not reliable. |
| winget prints text with no format contract. A winget update or a non-English Windows can break a parser. | Parse by column position, not by header words. Keep a recorded-output fixture per supported winget version. On a parse failure, show an indeterminate progress bar and let the exit code decide. |
| Two installs of different packages run together and Windows Installer refuses the second one (error 1618). | One queue for operations that change the system; WP6 (§6.7 gap table). |
| A second app instance uses the same ledger with a separate lock. | One app instance only; WP6 (§6.7 gap table). |
| Remote icons and screenshots send the user's IP address to other hosts. | A content security policy that permits HTTPS images only. The image proxy decision is in WP8. |
| A local web API that installs software can be reached by a web page in a local browser. | §6.5: nothing is built before a decision record. |

---

## 3. Repository layout (target)

The tree shows the target layout. A comment "WPn" marks a path that does not exist before that
work package. A path with no such mark exists. `CLAUDE.md`, "Layout (as built)", describes the
built files in more detail.

The update scan and update-all live in `ipc/commands.rs` (`run_check_updates`,
`run_update_all`) until WP6 moves them to `services/upgrade.rs`. `ui/src/routes/` holds `Search`
and `AppDetail` only.

```text
src-tauri/
  Cargo.toml
  tauri.conf.json
  src/
    main.rs            # entry: Tauri builder, plugin registration, logging init
    lib.rs             # module list and command registration
    error.rs           # AppError, display, serialization
    config.rs          # user config load/save, defaults
    domain/
      package.rs       # PackageSummary, PackageDetail, SourceKind, PackagingKind
      source.rs        # source record types
      install.rs       # InstallState and its legal transitions, UpdateAvailable
      progress.rs      # ProgressEvent (fields in §4.2)
    db/
      mod.rs           # pool init
      migrations/      # ordered SQL migrations
      models.rs        # row <-> domain mapping
      repo/            # repositories: installs, operation_log, packages, settings, sources
    catalog/
      mod.rs           # CatalogSource trait; search coordinator and cache
      search.rs        # query model
    sources/
      winget_repo.rs   # community catalog via REST (pending ADR 0001)
      msstore.rs       # WP7: winget msstore source, through the winget CLI only
      scoop.rs         # WP9
      choco.rs         # WP9
    executor/
      mod.rs           # PackageExecutor trait, OpGuard
      winget_cli.rs    # winget subprocess wrapper, process-tree stop
      winget_parse.rs  # progress parser, exit-code map, table parser
      package_manager.rs # WP11: local MSIX file (deferred, §2.1)
      scoop_cli.rs     # WP9
      choco_cli.rs     # WP9
    services/
      install.rs       # install/remove/cancel lifecycle, restart recovery
      upgrade.rs       # WP6: update scan, update one/all
      library.rs       # WP6: installed app list
      schedule.rs      # WP11: scheduled update scan
    ipc/
      commands.rs      # every #[tauri::command] handler
      events.rs        # event names + payload types
      state.rs         # AppState dependency graph
      tests.rs         # handler tests
      test_support.rs  # shared test fakes: FakeSource, FakeExec
    platform/
      windows.rs       # WP11: elevation, toast, scheduled task
    server/            # WP10: deferred (§6.5)
  tests/               # integration tests; fixtures/fake_winget.rs is the fake winget
ui/
  src/
    lib/               # typed API client for Tauri commands + event subscription
    components/        # AppCard, DetailHeader, InstallButton, ProgressBar, SourceBadge,
                       # RatingStars, UpdateRow, BuyInStoreButton, SourcePicker
    routes/            # Search, AppDetail; WP6: Library, Updates, Settings
  index.html
docs/
  adr/                 # short decision records
  designs/             # design documents
  reviews/             # review results
```

---

## 4. Domain & data model

### 4.1 Enumerations
- `SourceKind`: WingetCommunity, MsStore, Scoop, Chocolatey.
- `PackagingKind`: Msix, MsixBundle, Exe, Msi, Portable, Unknown.
- `InstallState`: NotInstalled, Installing, Installed, UpdateAvailable, Uninstalling, Failed, Pinned.
  `UpdateAvailable` and `Pinned` are reserved: no code writes them before WP6 (review action
  A10).
- `OperationStage`: queued, resolving, downloading, installing, finalizing, done, failed, cancelled.

### 4.2 Core structs (fields only)
- `PackageSummary`: id (stable, e.g. `src/pkg`), name, publisher, latest version, one-line
  description, icon url, rating, rating count, source kind, is_paid, packaging kind,
  store product id (optional; WP7 adds it).
- `PackageDetail`: everything in Summary plus full description, screenshots, website, license,
  release notes, installer metadata (architecture, install type, size, sha256), map of
  available versions.
- `ProgressEvent`: operation id, package id, operation, stage, fraction (0.0–1.0), message,
  timestamp.
- `UpdateAvailable`: package id, installed version, latest version, source.
- `OperationStarted`: operation id.
- `InstallStatus`: package id, state, installed version, target version, pinned, last updated,
  and the active operation (operation id and kind) when one runs.
- `OperationLogEntry`: id, time, level, operation, package id, message, detail.
- `UpdatesSummaryEvent`: total, succeeded, failed, and one outcome per package (package id, ok,
  message).

### 4.3 SQLite schema (tables)
- `packages` — cached catalog rows; key `(source, package_id)`; `fetched_at` for TTL.
- `installs` — local install ledger: package id, state, installed_version, target_version,
  pinned, last_update_check, last_updated. The table also has an `auto_update` column; no
  work package defines automatic updates, and no code may use the column.
- `sources` — enabled/disabled, priority, kind, extra config, last_sync.
- `settings` — key/value: theme, cache_ttl, update interval, default source exclusions.
- `operation_log` — id, ts, level, operation, package_id, message, detail (for UI "log" pane).

Rules: SQLite used only for cache + local state (never auth); all DB access behind async
repository functions; migrations are additive and versioned.

---

## 5. Service contracts

### 5.1 `CatalogSource`
Methods: `search(query)`, `get_detail(package_id)`,
`get_featured(limit)`, `list_categories()`, `capabilities()`.
Every method returns our domain types and an `AppError`. Sources declare a `SourceKind` and a
human display name. Rate limits and timeouts live inside each adapter.

### 5.2 `PackageExecutor`
Methods: `install(package, request) -> stream of ProgressEvent`, `cancel(operation_id)`,
`remove(package) -> stream of ProgressEvent`, `list_installed()`, `list_updates()`,
`update(package) -> stream`, `pin(package)`, `unpin(package)`, `is_available()`.
The executor is chosen by `SourceKind`. v1 has one executor.
Only `install`, `remove`, `update`, `pin` and `unpin` may change the system. `list_installed`,
`list_updates` and `is_available` are read-only: they must not install, update or remove
anything.

### 5.3 Coordinator
- Search fans out to all enabled `CatalogSource`s concurrently (bounded concurrency), merges and
  dedupes by normalized name+publisher, sorts, applies cache TTL, paginates.
- Install/update delegates to the executor implied by the package's source/packaging kind.

### 5.4 IPC commands and events (WP4 and WP5, built)

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
disable. The consent option of §6.6 is also not built.

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

## 6. Data-flow walkthroughs

### 6.1 Search (WP4, built)
1. The frontend calls `search_sources(search_id, query)`.
2. The core checks the cache for each enabled source and asks each stale source. The cache
   rule is pending ADR 0001: the cache keeps the stale rows of bug F1 for its time limit.
3. The core merges the results to `Vec<PackageSummary>` and stores the fetched rows in
   `packages`. The result returns from the call. The same result also arrives as
   `store:search-results`, or an error as `store:search-failed`, with the same `search_id`. The
   UI uses the `search_id` to drop a late answer to an old query.
4. Frontend renders cards; clicking one calls `get_app_detail`.

### 6.2 Install (community app) (WP5, built; step 2 has a planned part)
1. `start_install(package_id)` reads the detail, refuses a paid item, checks the ledger (§6.7),
   starts winget, sets `InstallState::Installing` and opens a `ProgressEvent` stream.
2. The executor runs a silent, non-interactive winget install with an exact id match and a
   named source. *Planned (§6.6):* it passes an agreement flag only directly after the user
   accepted that agreement. *On the branch:* the flags come from a config value.
3. winget's stdout/stderr is parsed into staged progress; each parse updates the stream.
4. The exit code decides the result, and §6.7 gives the ledger state for each result. The
   stream ends with one terminal event.
5. Frontend shows progress bar + a log tail; `cancel_install` stops the winget process tree
   (§6.7).

### 6.3 Update all (WP6, planned; the two commands are built)
1. `check_updates()` lists the available updates and changes nothing on the system. It runs
   `winget upgrade` with no package argument and no `--all`; that form only prints the list.
   Never use `--all` in a check: `winget upgrade --all` installs every update.
2. *Planned:* in v1 each read-only winget call names the `winget` source. A read-only call
   passes no agreement flag unless §6.6 permits it for that one call. *On the branch:*
   `check_updates` names no source.
3. Returns `Vec<UpdateAvailable>`; frontend shows list + "Update all".
4. `update_all()` runs the updates sequentially, emitting progress per package; results
   aggregated into a summary event. It skips a package that needs a package agreement and
   records it in the summary.

### 6.4 Purchase hand-off (WP5, built; never in-app)
1. A detail row with `is_paid=true` renders a **Buy in Microsoft Store** button instead of
   Install.
2. Core's `open_store_page(product_id)` calls the OS to open the official product URL in the
   default browser. Nothing else happens in-app.
3. A community package is never paid. An msstore item is paid, free or unknown; an unknown
   item gets the hand-off only. WP7 defines how the adapter reads the value (review action
   A17).

### 6.5 Headless mode (WP10, planned)

Blocked. Do not implement. A decision record must first answer `docs/STRATEGY.md` open
question 6: the hosting model, authentication in each mode, origin checks, consent over HTTP
(§6.6), and winget on Windows Server.

### 6.6 Agreement consent (WP5, planned)

Hard rule 4 applies. The app never accepts an agreement that the user did not see.

#### Rules

- **The app stores no consent.** No setting, no database column and no file holds an
  acceptance. winget can keep an acceptance itself; the app does not depend on that.
- **An agreement flag goes to winget for one operation only, and only directly after the user
  accepted that agreement in the consent dialog.**
- When winget asks again, on a new machine or after a change of terms, the app shows the
  dialog again.
- No transport other than the desktop IPC may carry a consent until the WP10 decision record
  defines it.

| Kind | When the user sees it | Storage in the app |
|---|---|---|
| Source agreement | Each time winget reports that it is necessary. | None. |
| Package agreement | Each time winget reports that it is necessary. | None. |

#### Spike (the first WP5 task)

Three facts about winget are not verified. Answer them on a real machine before other work on
this section, and record the answers in the `winget-executor` skill:

1. Where does the agreement text come from when winget runs with no interactive prompt? The
   output of `winget show` is the first candidate.
2. Does winget keep a source acceptance after one accepted call?
3. Does winget ask again when the terms of a source change?

The rules above are correct for each answer. The answers decide only how frequently the
dialog opens and what it can show.

Fallback when winget gives no text: the dialog names the package, the source and the kind of
agreement, and shows the agreement or license link from the package data. The link opens in
the browser. **This fallback needs maintainer approval:** the maintainer decides if a link
satisfies hard rule 4. Until that approval, a case with no text stops the operation with a
clear message.

#### Flow

1. An operation starts with no agreement flag.
2. When winget needs an agreement, the operation ends with the error `agreements_required`.
   One field of the error says which kind is missing. The ledger goes back to the state it
   had before the operation started (§6.7, the "winget needs an agreement" row).
3. The UI opens the consent dialog and shows no error. The dialog names the package, its
   version and the source, says which kind of agreement is necessary, and shows the agreement
   text.
4. **Accept.** The UI calls the start command again with `options.consent`: the kinds that the
   user accepted in the dialogs of this operation, and the package version that the dialog
   showed. The core passes the flag of each accepted kind for this one operation and installs
   exactly that version. If that version is not available, the operation ends with
   `agreements_required` again.
5. When winget then needs the second kind, steps 2 to 4 repeat. The dialog opens two times at
   most for one operation.
6. **Decline.** Nothing more runs. The core writes one Info row to `operation_log`:
   "agreements declined".

A read-only command (`check_updates`, a library list) that ends with `agreements_required`
returns that error. The UI shows the dialog for the source agreement. After "Accept", the UI
calls the command again with the source consent, and the core passes the source flag for that
one call.

#### Changes that WP5 owns

- Remove `accept_source_agreements` from `AppConfig`, the config type in `config.rs`. On the
  branch on 2026-11-11 it sets the source and package agreement flags for each operation,
  which is the silent acceptance that rule 4 forbids. It defaults to false, and no screen sets
  it, so no user loses a setting.
- Add `options.consent` to `start_install`, and the same option to `start_uninstall` and
  `check_updates` for the source kind.
- Add the "winget needs an agreement" row of §6.7 to the code. On the branch this case is
  recorded as a failure.
- Add the consent dialog. The UI must read `error.code`; on the branch it reads only
  `cancelled`.
- Update the `winget-executor` skill (hard rules, command lines, test list) to this section,
  in the same pull request.
- Tests: the fake winget returns each of the two agreement exit codes; an accepted retry
  passes only the accepted flag and the shown version; a decline and a first
  `agreements_required` leave the ledger as it was; no operation passes a flag with no
  consent.

### 6.7 Install state transitions (WP5, built)

#### Terms

- The *ledger* is the `installs` row.
- An operation is *active* while its winget process runs in this app process.
- The *rest state* of a package is `Installed` when a version is recorded (`Pinned` when the
  pin flag is set), and `NotInstalled` when no version is recorded.
- `Installing` and `Uninstalling` are the only busy states; an update uses `Installing`.

The legal edges are in `domain/install.rs`. An illegal edge returns the error `unsupported` and
changes nothing.

#### Start

The rows apply from top to bottom; the first match wins.

| Ledger state | Command | Result |
|---|---|---|
| Any, paid item | `start_install` | No change; `unsupported`. |
| Any, an operation is active for the package | Either | No change; `unsupported`. The message names the running operation. |
| `Uninstalling`, `Pinned` | `start_install` | No change; `unsupported`. |
| `Installing` with no active operation (a leftover) | `start_install` | `Installing`. |
| No row, `NotInstalled`, `Installed`, `UpdateAvailable`, `Failed` | `start_install` | `Installing`; `target_version` set. |
| No row, `NotInstalled`, `Installing`, `Uninstalling` | `start_uninstall` | No change; `unsupported`. |
| `Installed`, `UpdateAvailable`, `Failed`, `Pinned` | `start_uninstall` | `Uninstalling`. |
| Any, winget is absent | Either | No change; `winget_not_found`. |

A second click during an operation and an uninstall during an install are the "operation is
active" row. The UI also disables the buttons while an operation runs.

#### End of an operation

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
| Any | winget needs an agreement (*planned, §6.6*) | `failed` | The state before the operation started | Info | `agreements_required` |

- A row that has the pin flag goes back to `Pinned` where the table says `Installed`.
- The exit code map is in the `winget-executor` skill.
- On the branch, "winget needs an agreement" follows the failure rows: `Failed` and an Error
  row for a first install. WP5 changes it to the row above.
- `Failed` after a failed uninstall keeps the version. The app is probably still installed,
  and the ledger cannot say so. A failed install with a version keeps `Installed`, because the
  earlier version is still there. The gap table has the fix for the uninstall case.

#### Cancel

- Cancel does the same thing in each stage: it stops the winget process tree. The app uses a
  Job Object first and `taskkill /T /F` second. winget has no cancel call.
- When the process ended before the cancel arrived, the real outcome stands. An exit code of 0
  after a cancel is a success.
- Cleanup after a cancel means: the process tree is stopped, `target_version` is cleared, and
  the ledger is in the rest state. The row stays. The app does not delete downloaded files
  and does not undo an installer that already started to write; winget and the installer own
  those files.
- When the app cannot stop the tree (an elevated installer), the operation continues and its
  real outcome settles the ledger. The executor writes a Warn log row. A second cancel for
  the same operation does nothing.

#### Restart

On the first use of the core after the app starts, each ledger row in `Installing` becomes
`Installed` when a version is recorded and `Failed` when not. Each ledger row in `Uninstalling`
becomes `Failed`. The core writes one Warn entry to `operation_log` for each changed ledger
row. It does not look for a winget process that survived the last run.

#### Known gaps

The review ids are from `docs/reviews/2026-11-11-spec-review.md`.

| Review id | Gap | Planned fix | Owner |
|---|---|---|---|
| none | A cancel that cannot stop the tree is not reported to the UI. The Cancel button stays and does nothing. | Emit a `store:progress` event with a message, and disable the button. | WP5 |
| A2 | No agreement consent. | §6.6. | WP5 |
| none | `check_updates` names no source, so it queries each configured source. | Name the `winget` source (§6.3). | WP5 |
| A24 | "An operation is active" returns `unsupported`, and the UI cannot separate it from other refusals. | Add the code `busy`, retryable. | WP5 |
| none | The recorded version is the catalog target, not the version that winget installed. | After a success, read the version from the installed list. Keep the target as the fallback and write a Warn row on a difference. | The F1 fix |
| A10 | After a failed uninstall and after a restart, the ledger guesses. | Check the installed list and set `Installed` or `NotInstalled` from it. Keep `Failed` for "install tried, no version". | WP6 |
| A10 | Nothing writes `Pinned` or `UpdateAvailable`; no pin command exists. | Pin becomes a flag; update-available comes from the scan. | WP6 |
| A11 | Two installs of different packages can run together. | One queue for each operation that changes the system. Installer exit 1618 maps to a retryable busy error. | WP6 |
| none | A second app instance has a separate lock on the same ledger. | One app instance only. | WP6 |
| A13 | The exit code is not stored in `operation_log`. The "nothing to do" exit code counts as success for install, update and uninstall. | An `exit_code` column; a rule per operation. | WP6 |
| A24 | A paid item, an illegal transition, a timeout, a rate limit and a busy installer have no code of their own. | New codes. | WP6 |
| none | A service-layer failure to settle the ledger goes to the trace log only. | Write an Error row to `operation_log`. | WP6 |
| A12 | Cancel does not depend on the stage; an elevated installer cannot be cancelled. | Stage-dependent cancel. | WP11 |
| A24 | The UI never sets the install scope, so winget uses its default. | A scope choice and the elevation path. | WP11 |

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
winget; the consent tests of §6.6 pass; a cancelled install leaves the ledger in its rest
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