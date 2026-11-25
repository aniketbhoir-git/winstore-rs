# Store — implementation specification

> **Objective:** Store is an open-source desktop app for Windows that finds, installs, updates
> and removes applications through the Windows Package Manager (winget). This document defines
> how the app works: its rules, its data, its commands and its work packages. It contains no
> code.

> **Status of this document (2026-11-13):** WP0 to WP4 are on `main`. WP5 is partly built: its
> code is on branch `impl/6--install-loop` (PR #19, merged 2026-11-25). The spike of §6.6 is done
> (`docs/spikes/2026-11-13-agreement-consent.md`); the maintainer selected case (c) for WP5 on 2026-11-13. The core
> work of WP5 (step 2: case (c) of §6.6 and the WP5 rows of the gap table in §6.7) is built on
> the branch. Step 3 (the UI work) is built on branch `impl/6--ui-step3` (second PR, 2026-11-25). Maintainer decision of
> 2026-11-12: the UI part goes into a second pull request, after the approval of
> `docs/designs/unattended-testing.md` (§7, WP5). Bug F1 (#20) does not gate PR #19; it gates the MVP
> milestone (§7). WP6 to WP11 are not started. §7 gives the status of each work package.
>
> - A section marked *built* describes code on `main`. A section marked *built on branch*
>   describes code that is only on `impl/6--install-loop`; for that section the branch is the
>   reference.
> - A *built* or *built on branch* section is the contract for that code. Where it and the
>   code disagree, the code is the fact: stop, report the difference, and correct this
>   document. Do not rebuild code that exists.
> - A section or a sentence marked *planned* is the plan. It overrides the code.
> - winget flags, exit codes and output formats are in the `winget-executor` skill
>   (`.claude/skills/winget-executor/SKILL.md`), not here.
> - Open review actions (ids A1 to A25) are in `docs/reviews/2026-11-11-spec-review.md`. An id
>   such as "A5" in this document names one action in that file; the text next to the id says
>   what the action decides.

## Problem

Windows has a package manager, winget, that installs, updates and removes applications. It has
only a command line. The Microsoft Store app gives a graphical catalog, but it shows ads and
asks for an account, and it is absent or unwanted on Windows Server and on reduced Windows
installs. Three kinds of user need a graphical front end for winget
(`docs/STRATEGY.md` §1.3):

1. A desktop user who wants to browse, install, update and remove free apps.
2. An owner of a home server or a headless machine that has no Store app (a later target).
3. An administrator who installs a fixed set of apps and needs clear logs.

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

Terms of this document. Each term has one meaning:

- A *catalog source* is an adapter in the app that implements `CatalogSource` (§5.1).
- A *winget source* is a source that winget itself knows, such as `winget` or `msstore`. A
  *winget source agreement* belongs to a winget source.
- The *ledger* is the `installs` row of a package.
- An *operation* is one winget run that changes the system: one install, one update or one
  uninstall. It has one `operation_id`.
- An *attempt* is the chain of operations for one user action that consent retries link
  (§6.6). An attempt has one operation, or two or three when winget needs agreements.
- An operation is *active* while its winget process runs in this app process.
- The *rest state* of a package is `Installed` when a version is recorded (`Pinned` when the
  pin flag is set), and `NotInstalled` when no version is recorded.
- `Installing` and `Uninstalling` are the only *busy states*; an update uses `Installing`.
- A cache row is *expired* when it is older than the cache time limit. "Stale" is used only
  for the out-of-date rows of bug F1.

Out of scope, never to be implemented or researched further: payment, account sign-in, and
any licensing/entitlement or content-protection system. When a paid item appears, the only
supported action is the official browser hand-off.

Hard rules:

1. No payment, account sign-in, or licensing/entitlement handling of any kind. Any item flagged
   `is_paid=true` shows only a call-to-action that opens the official product page in the
   user's default browser: `https://apps.microsoft.com/detail/{productId}`. The app uses this
   HTTPS address and no other.
2. Catalog sources only ever read public, machine-readable data over HTTPS. Never touch or
   attempt to authenticate against or scrape account/proprietary service internals.
3. The product name and branding are original. Do not use Microsoft logos or artwork. The text
   label "Buy in Microsoft Store" on the hand-off button is permitted.
4. Every install, update, uninstall and pin must originate from an explicit user click and
   respect winget's own source/package agreements (surface them; never pre-accept silently).
   §6.6 defines the consent flow.
5. No telemetry. The app makes network calls only to these destinations:
   - the catalogs that the user turned on;
   - winget, which makes its own calls;
   - HTTPS image hosts that the data of an enabled catalog names (icons and screenshots),
     until the WP8 image decision.

   The hand-off of rule 1 opens the default browser; the app itself makes no call there.

---

## 1. System overview

A desktop application for Windows that finds, installs, updates, pins, and removes applications,
backed by a pluggable set of *catalog sources* and *executors* (installs/removals). Before WP9
the app has one executor, the winget CLI.
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

The message of `ElevationRequired` is: "This app needs administrator rights to install. Windows did not give them. Try again and accept the Windows prompt, or choose 'Only for me' if the app supports it."

- Every error maps to: (stable string `code`, human message, optional raw detail for logs,
  is_retryable flag). IPC returns this as JSON; logs record the raw detail.
- In one `AgreementsRequired` error, exactly one of the two fields is true. winget reports one
  kind of agreement at a time. *Built on the branch (WP5):* the JSON of `agreements_required`
  carries `source_agreements` and `package_agreements`, and the data for the consent dialog
  of §6.6:
  - `winget_source`: the name of the winget source that the operation used.
  - `version`: the version that the operation gave to winget. When it gave none, the latest
    version of the cached detail. `null` only when the core has no version. *On the branch:*
    an uninstall gives `null`, and `update_all` gives the latest version of its update scan.
  - `agreement_text`: the agreement texts that winget printed, joined, or `null`.
  - `agreement_url`: the first https link that winget printed, or `null`. For a package
    agreement with no link in the output it is the `license_url` of the package detail (§4.2).
  - `agreements`: every agreement that winget printed, as `{ label, text, url }` (`text` and
    `url` can be `null`; `url` is https only). The dialog shows these. *Built, F4.*
  - `consent_token`: a handle that binds a later consent to this package, version, winget
    source and kind (§6.6 "How consent is bound"), or `null` (an update gets none). *Built, F4.*

  The executor fills `winget_source` and `version` (the requested version) before the operation
  ends, and parses `agreement_text`, `agreement_url` and `agreements` from the agreement block
  of winget's output (`ProgressParser`). The service (`services/install.rs`) fills `version`
  with the latest version of the detail when the request gave none, and issues
  `consent_token`. *Built, F4 (case (b)).*

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
| Windows APIs | official `windows` crate (notifications, AppModel). Features in use: `Win32_*` for elevation and job objects, `UI_Notifications` and `Data_Xml_Dom` for toasts (Â§6.9). |
| UI tests (dev only) | Vitest with `jsdom`, `@testing-library/svelte` and `mockIPC` from `@tauri-apps/api/mocks`. Dev dependencies of `ui/`; none ships in the app. See §8 and `docs/designs/unattended-testing.md`. |
| Install/remove engine | winget CLI invoked as a subprocess for install/update/remove |
| Logging | tracing; operation log mirrored into SQLite |
| Minimum Windows version | Windows 11 23H2 (build 22631). Maintainer decision of 2026-11-12: the minimum is the version on the maintainer devices. Only one device was read; a lower build on another device lowers this value. |
| Minimum winget version | 1.29. Same decision. The code and the `winget-executor` skill check 1.29 (built in #20 step 2). |

Rationale: winget handles installer execution, hash checks, agreements, dependencies and MSIX
deployment. The app wraps it and does not write installer logic.

### 2.1 Deferred, needs a decision

These items are not decided. Do not build them and do not add their dependencies.

| Item | Work package | Waits for |
|---|---|---|
| Headless mode and its web server (axum was the first idea) | WP10 | A decision record (review action A5) |
| A second executor: `Windows.Management.Deployment.PackageManager` for a local MSIX file | WP11 | Review action A9. Before WP9 the app has one executor. |
| Scoop and Chocolatey sources and executors | WP9 | WP9 start |

### 2.2 Alternatives considered

| Decision | Options | Status |
|---|---|---|
| UI shell | (a) Tauri 2; (b) a native Rust toolkit, iced or Slint | Decided: (a). It is the fastest route to a card-based store UI and has plugins for notifications and updates. (b) gives a smaller binary and no web code, but a rich UI is much more work. Detail: `docs/STRATEGY.md` §2.3. |
| Community catalog backend | (a) the `winget.run` REST mirror, in the code today; (b) `winget search` and `winget show` through the executor; (c) the official winget index file; (d) the winget COM API; (e) a catalog that the project builds | Proposed: (b). (a) returns stale rows. (c) has no documented format. (d) needs generated bindings, a dependency outside §2, and access from an app with no package identity is not verified. (e) makes the project the owner of a data pipeline and a host with one maintainer; it stays a later option. Detail: `docs/adr/0001-community-catalog-backend.md`. |
| Executor interface | (a) the winget CLI, built; (b) the winget COM API `Microsoft.Management.Deployment` | (a) is built and stays. (b) gives typed progress and a cancel call, and removes text parsing. It needs generated bindings, and access from an app with no package identity is not verified. Open; decide before WP6. The consent flow of §6.6 depends on the CLI exit codes; a pick of (b) reopens §6.6. |
| Headless shape | (a) a local web server; (b) a command-line tool; (c) remote desktop only | Open (review action A5). No option has a recorded trade-off yet. (a) adds a local API that a web page can reach (§2.3). The decision record must compare the three. |

### 2.3 Risks and mitigations

`docs/STRATEGY.md` §5 holds the project risks. This table holds the risks that this design
adds.

| Risk | Mitigation or owner |
|---|---|
| The app accepts an agreement that the user did not see. | §6.6: the app stores no consent and passes a flag only directly after a user "Accept". |
| winget gives no agreement text without an interactive prompt, so the consent dialog has nothing to show. | The spike in §6.6 comes first in WP5. The spike of 2026-11-13 found agreements with a link and no text. WP5 builds case (c): Store does not install such a package. The link fallback of case (b) is later work (backlog F4). |
| The third-party catalog is stale or wrong (bug F1). | ADR 0001. Until it is accepted, the search results are not reliable. |
| winget prints text with no format contract. A winget update or a non-English Windows can break a parser. | Parse by column position, not by header words. Keep a recorded-output fixture per supported winget version. On a parse failure, show an indeterminate progress bar and let the exit code decide. |
| Two installs of different packages run together and Windows Installer refuses the second one (error 1618). | One queue for operations that change the system; WP6 (§6.7 gap table). |
| A second app instance uses the same ledger with a separate lock. | One app instance only; WP6 (§6.7 gap table). |
| A script in the webview sends a consent that no user gave. The core cannot tell it from a real "Accept". | §6.6, "Trust boundary": a script policy and text-only catalog data; WP5. |
| With ADR 0001 option (b), the first `winget show` for a package took 21 to 23 s on the one machine that was measured. The first Install click can wait that long. | `start_install` uses the cached detail when one exists (§6.2). The ADR 0001 work measures a second machine and sets the time limit. |
| Remote icons and screenshots send the user's IP address to other hosts. | Hard rule 5 permits these loads over HTTPS only. A content security policy enforces HTTPS. The image proxy decision is in WP8. |
| A local web API that installs software can be reached by a web page in a local browser. | §6.5: nothing is built before a decision record. |

---

## 3. Repository layout (target)

The tree shows the target layout. A comment "WPn" marks a path that does not exist before that
work package. A path with no such mark exists. `CLAUDE.md`, "Layout (as built)", describes the
built files in more detail. The tree does not show paths for the deferred items of §2.1.

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
    executor/
      mod.rs           # PackageExecutor trait, OpGuard
      winget_cli.rs    # winget subprocess wrapper, process-tree stop
      winget_parse.rs  # progress parser, exit-code map, table parser
    services/
      install.rs       # install/remove/cancel lifecycle, restart recovery
      upgrade.rs       # WP6: update scan, update one/all
      library.rs       # WP6: installed app list
      schedule.rs      # WP11: scheduled update scan
      export.rs        # WP11 step 5: export of the installed packages (built on branch)
      notify.rs        # WP11 step 3: toast on finish (built)
    ipc/
      commands.rs      # every #[tauri::command] handler
      events.rs        # event names + payload types
      state.rs         # AppState dependency graph
      tests.rs         # handler tests
      test_support.rs  # shared test fakes: FakeSource, FakeExec
    platform/
      windows.rs       # WP11: elevation check (built, step 1), toasts (built, step 3), scheduled task
  tests/               # integration tests; fixtures/fake_winget.rs is the subprocess fake
ui/
  src/
    lib/               # typed API client for Tauri commands + event subscription
    components/        # AppCard, BuyInStoreButton, DetailActions, DetailHeader,
                       # InstallButton, LogTail, ProgressBar, RatingStars, Screenshots,
                       # SourceBadge; WP6: UpdateRow, SourcePicker
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
  `UpdateAvailable` and `Pinned` are reserved. No code path reaches them before WP6: nothing
  sets the pin flag, and nothing writes the result of the update scan (review action A10).
- `Operation` (the operation kind): install, remove, update. "Remove" is the uninstall.
- `OperationStage`: queued, resolving, downloading, installing, finalizing, done, failed, cancelled.

The values of `Operation` and `OperationStage` are shown as their wire strings.

### 4.2 Core structs (fields only)
- `PackageId`: one string, `{source kind}/{id in that source}`, split at the first `/`. Each
  `package_id` argument and field in this document has this form. The cache key of §4.3 holds
  the two parts in separate columns.
- `PackageSummary`: id (a `PackageId`), name, publisher, latest version, one-line
  description, icon url, rating, rating count, source kind, is_paid, packaging kind,
  store product id (optional; WP7 adds it).
- `PackageDetail`: everything in Summary plus full description, screenshots, website, license,
  release notes, installer metadata (architecture, install type, size, sha256), map of
  available versions, and `license_url` (optional; https only; *built, F4*): the `License Url`
  of `winget show`. It only feeds `agreement_url`.
- `ProgressEvent`: operation id, package id, operation kind, stage, fraction (0.0–1.0),
  message, timestamp, `cancel_failed` (boolean, default false; *built on the branch*,
  §6.7 gap table) and `indeterminate` (boolean, default false). `indeterminate` is true while
  winget has shown no download bar for the operation (since winget 1.29.240 it prints none to a
  pipe). `fraction` then is only a stage estimate that keeps the stream monotonic; the UI draws
  an animated bar with no value and shows `message` (winget's stage line). It turns false with
  the first bar line, and on a `done` event (fraction 1.0). A `failed` or `cancelled` event
  keeps the last value of the flag.
- `UpdateAvailable`: package id, installed version, latest version, source, is_auto. `is_auto`
  is reserved: no work package defines automatic updates.
- `OperationStarted`: operation id.
- Install options (the `options` argument of §5.4): version (optional), scope (optional;
  `user` or `machine`; the UI does not set it before WP11), and consent (*built, F4; §6.6*).
- Consent (*built, F4; §6.6*), as JSON: `{ "kinds": [...], "tokens": [...] }`.
  - `kinds`: a list with `"source"`, `"package"`, or the two. An empty list is the same as no
    consent.
  - `tokens`: the `consent_token` of each `agreements_required` error that the user accepted
    (one for each kind). The core refuses a kind that no token of the same package, version,
    winget source and kind backs (`invalid_config`; §6.6 "How consent is bound").
  - The consent holds no version. The one version field is `options.version`.
  - `start_install` needs `options.version` when `kinds` has `"package"`; with no version it
    returns `invalid_config`. The UI sets it to the `version` field of the
    `agreements_required` error (§1.3). With the kind `"source"` only, `options.version`
    stays optional.
  - `start_uninstall` accepts the kind `"source"` only. The kind `"package"` there returns
    `invalid_config` (`winget uninstall` accepts the source agreement flag only; spike
    question 3).
- `InstallStatus`: package id, state, installed version, target version, pinned, last updated,
  and the active operation (operation id and kind) when one runs.
- `OperationLogEntry`: id, time, level, operation kind, package id, message, detail.
- `UpdatesSummaryEvent`: total, succeeded, failed, and one outcome per package (package id, ok,
  message).

### 4.3 SQLite schema (tables)
- `packages` — cached catalog rows; key `(source, package_id)`; `fetched_at` for TTL.
- `installs` — local install ledger: package id, state, installed_version, target_version,
  pinned, last_update_check, last_updated. The table also has an `auto_update` column with the
  default true. No work package defines automatic updates, and no code may act on the column.
- `sources` — enabled/disabled, priority, kind, extra config, last_sync.
- `settings` — key/value: theme, cache_ttl, update interval, default source exclusions. Keys
  of WP11: `notifications_enabled` (`"false"` turns toasts off, §6.9), `update_scan`
  (`off`, `daily`, `weekly`), `update_scan_last`, `update_scan_last_ids` (§6.10).
- `operation_log` — id, ts, level, operation, package_id, message, detail (for UI "log" pane).

Rules: SQLite used only for cache + local state (never auth); all DB access behind async
repository functions; migrations are additive and versioned.

---

## 5. Service contracts

### 5.1 `CatalogSource`
Methods: `search(query)`, `get_detail(package_id)`,
`get_featured(limit)`, `list_categories()`, `capabilities()`.
Every method returns our domain types or an `AppError`. Catalog sources declare a `SourceKind` and a
human display name. Rate limits and timeouts live inside each adapter.

### 5.2 `PackageExecutor`
Methods: `install(package, request) -> stream of ProgressEvent`, `cancel(operation_id)`,
`remove(package) -> stream of ProgressEvent`, `list_installed()`, `list_updates()`,
`update(package) -> stream`, `pin(package)`, `unpin(package)`, `is_available()`.
The executor is chosen by the `SourceKind` of the package. Before WP9 the app has one
executor.
Only `install`, `remove`, `update`, `pin` and `unpin` may change the system. `list_installed`,
`list_updates` and `is_available` are read-only: they must not install, update or remove
anything.

### 5.3 Coordinator
- Search fans out to all enabled `CatalogSource`s concurrently (bounded concurrency), merges and
  dedupes by normalized name+publisher, sorts, applies the cache time limit, paginates.
  *Planned (WP7, when a second catalog source exists):* a merged card keeps the `PackageId`
  of the catalog source with the highest priority. That id decides the executor and the
  winget source of an install.
- Install/update delegates to the executor that §5.2 selects.

### 5.4 IPC commands and events (WP4 built; the WP5 rows built on branch)

All handlers are in `ipc/commands.rs`. Each one wraps a plain async function that the tests in
`ipc/tests.rs` call. A new command needs a row here in the same pull request.

| Command | Arguments | Returns | Behavior |
|---|---|---|---|
| `app_version` | none | string | The crate version. |
| `app_status` | none | `{ elevated: bool }` | True when Store runs as administrator (§6.8). |
| `search_sources` | `search_id`, `query` | `Vec<PackageSummary>` | Catalog search. Also emits `store:search-results` or `store:search-failed` with the same `search_id`. |
| `get_app_detail` | `package_id` | `PackageDetail` | Catalog detail. |
| `start_install` | `package_id`, `options?` (`version?`, `scope?`, `consent?`; *built, F4*) | `OperationStarted { operation_id }` | Refuses a paid item. Checks the ledger, checks `consent` against the open agreement challenges (§6.6), starts winget, sets `Installing`. |
| `start_uninstall` | `package_id`, `options?` (`consent?`, kind `source` only; *built, F4*) | `OperationStarted` | Needs a ledger row. Checks `consent` as above. Sets `Uninstalling`. |
| `open_agreement_link` | `package_id`, `url` (*built, F4*) | nothing | Opens `url` in the system browser, only when it is a link of a live agreement challenge of that package (§6.6); otherwise `invalid_config`. |
| `cancel_install` | `operation_id` | nothing | Cancels an install, an uninstall or an update. An unknown or finished id is not an error. |
| `get_install_status` | `package_id` | `InstallStatus` | The ledger row and the active operation, if one runs. No row gives `NotInstalled`. |
| `get_operation_log` | `package_id`, `limit?` | `Vec<OperationLogEntry>` | The log tail, oldest first. Default 100 rows, maximum 500. |
| `check_updates` | none | `Vec<UpdateAvailable>` | Read-only list (§6.3). Does not write the ledger. |
| `update_all` | none | `UpdatesSummaryEvent` | Sequential updates with progress. The command exists; WP6 adds its UI and finishes its behavior (§6.3). |
| `get_update_scan` | none | string | The saved update-scan mode: `off`, `daily` or `weekly` (*built on branch, WP11 step 4*; §6.10). |
| `set_update_scan` | `mode` | nothing | Saves the mode. Any other text: `invalid_config`, nothing saved (*built on branch, WP11 step 4*; §6.10). |
| `open_store_page` | `product_id` | nothing | Checks the id, then opens the official product page in the browser. |
| `export_packages` | none | `ExportResult { path, file_name, package_count }` | Runs `winget export -o <temp> --source winget --disable-interactivity`, checks the file with the import parser, writes it through a temp file and a rename to `Documents\Store\exports\store-export-<date>.json` (*built on branch, WP11 step 5*; §6.11). Never an agreement flag; `agreements_required` writes nothing. |
| `open_exports_folder` | none | nothing | Creates the exports folder if needed and opens it with `%WINDIR%\explorer.exe` (full path) (*built on branch, WP11 step 5*). The path is the core's own. |
| `read_import_file` | `path` | `ImportReview { file_name, rows: [{ package_id, version?, source, status }], installed_known }` | Reads a winget export file (*built on branch, WP11 step 6*; §6.11). `path` must be a path the core recorded from a drop event, or a `.json` file in the exports folder; any other path gives `invalid_config` and the file is not opened. Installs nothing. `status` is `ready`, `installed`, `not_valid` or `source_not_supported`. |
| `start_server` | `port` | error `unsupported` | Stub until WP10. |

Event `store:import-dropped` (*built on branch, WP11 step 6*): `{ path, file_name }` for each file that the core recorded from a drop on the main window. `path` is the canonical path to give back to `read_import_file`.

Not built: pin and unpin commands, a library list, settings read and write, source enable and
disable.

| Event | Payload |
|---|---|
| `store:progress` | `ProgressEvent` |
| `store:search-results` | `search_id`, `results` |
| `store:search-failed` | `search_id`, `error` |
| `store:operation-finished` | `operation_id`, `package_id`, `ok`, `error?` |
| `store:updates-summary` | `total`, `succeeded`, `failed`, `outcomes` |

`store:progress` carries `indeterminate` (§4.2): with winget 1.29.240 and newer the stream holds
stage lines only (`Found …`, `Downloading <url>`, hash check, `Starting package install...`,
`Successfully installed`), each as `message`, and every event, a failed or cancelled
terminal event included, is `indeterminate`; only `done` is not.

For each operation, the last `store:progress` event carries the terminal stage. Then
`services/install.rs` settles the ledger. Then it emits `store:operation-finished`. A cancel gives `ok = false` with
the error code `cancelled`; the UI must not show that as a failure.

---

## 6. Data-flow walkthroughs

### 6.1 Search (WP4, built)
1. The frontend calls `search_sources(search_id, query)`.
2. The core checks the cache for each enabled catalog source and asks each catalog source
   whose cache rows are expired. The cache rule is pending ADR 0001: the cache keeps the stale
   rows of bug F1 for its time limit.
3. The core merges the results to `Vec<PackageSummary>` and stores the fetched rows in
   `packages`. The result returns from the call. The same result also arrives as
   `store:search-results`, or an error as `store:search-failed`, with the same `search_id`. The
   UI uses the `search_id` to drop a late answer to an old query.
4. Frontend renders cards; clicking one calls `get_app_detail`.

### 6.2 Install (community app) (WP5, built on branch; step 1 has a planned part)
1. `start_install(package_id)` reads the detail, refuses a paid item, checks the ledger (§6.7),
   starts winget, sets `InstallState::Installing` and opens a `ProgressEvent` stream.
   *Planned (with the ADR 0001 work):* it uses the cached detail when one exists, so that a
   slow catalog read does not delay the install (§2.3).
2. The executor runs a silent, non-interactive winget install with an exact id match and a
   named winget source. It passes no agreement flag in any case (§6.6, case (c)). *Planned (case
   (b), F4):* it passes an agreement flag only directly after the user accepted that agreement.
3. winget's stdout/stderr is parsed into staged progress; each parse updates the stream.
4. The exit code decides the result, and §6.7 gives the ledger state for each result. The
   stream ends with one terminal event.
5. Frontend shows progress bar + a log tail; `cancel_install` stops the winget process tree
   (§6.7).

### 6.3 Update all (WP6, planned; the two commands are built)
1. `check_updates()` lists the available updates and changes nothing on the system. It runs
   `winget upgrade` with no package argument and no `--all`; that form only prints the list.
   Never use `--all` in a check: `winget upgrade --all` installs every update.
2. *Planned (WP6):* before WP7 each read-only winget call names the `winget` winget source.
   WP7 decides which winget sources a read-only call names after that. A read-only call
   passes no agreement flag unless §6.6 permits it for that one call. *On the branch:*
   `check_updates` names no winget source.
3. Returns `Vec<UpdateAvailable>`; frontend shows list + "Update all".
4. `update_all()` runs the updates sequentially, emitting progress per package; results
   aggregated into a summary event. It skips a package that needs a package agreement and
   records it in the summary. *Planned (WP6):* when winget needs a winget source agreement,
   `update_all` stops before the first package and returns `agreements_required`; no package
   is updated. WP6 defines the consent step for this case (§6.6, "Read-only calls").

### 6.4 Purchase hand-off (WP5, built on branch; never in-app)
1. A detail row with `is_paid=true` renders a **Buy in Microsoft Store** button instead of
   Install.
2. Core's `open_store_page(product_id)` calls the OS to open the official product URL in the
   default browser. Nothing else happens in-app.
3. A community package is never paid. `is_paid` is a boolean and has no "unknown" value: the
   msstore adapter sets `is_paid = true` unless the catalog data shows that the item is free.
   An item with no price data thus gets the hand-off only. WP7 defines how the adapter reads
   the value (review action A17).

### 6.5 Headless mode (WP10, planned)

Blocked. Do not implement. A decision record must first answer `docs/STRATEGY.md` open
question 6: the hosting model, authentication in each mode, origin checks, consent over HTTP
(§6.6), and winget on Windows Server.

### 6.6 Agreement consent (WP5; case (c) built on branch; case (b) built as F4, 2026-11-25)

Hard rule 4 applies. The app never accepts an agreement that the user did not see.

#### Rules

- **The app stores no consent.** No setting, no database column and no file holds an
  acceptance. winget can keep an acceptance itself; the app does not depend on that.
- **An agreement flag goes to winget for one operation only, and only directly after the user
  accepted that agreement in the consent dialog.** The terms *operation* and *attempt* are in
  §0: one winget run is one operation; a consent retry starts a new operation in the same
  attempt.
- An acceptance is valid inside one attempt. The UI holds it in memory until the attempt ends
  and sends it with each retry of that attempt. A new attempt starts with no consent.
- When winget asks again, on a new machine or after a change of terms, the app shows the
  dialog again.
- No transport other than the desktop IPC may carry a consent until the WP10 decision record
  defines it.

| Kind | When the user sees it | Storage in the app |
|---|---|---|
| winget source agreement | Each time winget reports that it is necessary. | None. |
| Package agreement | Each time winget reports that it is necessary. | None. |

#### Trust boundary

The core cannot check that a user clicked "Accept". It trusts the webview. A script that runs
in the webview can thus send a consent that no user gave. WP5 must keep foreign script out of
the webview:

1. Set a content security policy in `tauri.conf.json` with `script-src 'self'` and no inline
   script. Built on the branch: `app.security.csp` and `devCsp` hold it; the test
   `src-tauri/tests/csp_policy.rs` checks it.
2. Show each piece of catalog data (name, description, release notes, agreement text) as
   text. Never put it into the page as HTML. On the branch the UI obeys this; a test must
   keep it so.
3. *Built, F4.* The agreement link is shown as text and opens only through
   `open_agreement_link` in the system browser; the UI has no anchor with an address.

#### How consent is bound (case (b), F4)

The core cannot see a click, but it can refuse a consent that no `agreements_required` asked
for. This keeps a script in the webview from reusing one acceptance for another package.

- When an install or uninstall ends with `agreements_required`, `services/install.rs` issues a
  *challenge*: a random 128-bit token, held in memory only (`services/consent.rs`), bound to the
  package id, the `version`, the winget source and the kind (source or package). The token is
  in the error as `consent_token`. An update gets none.
- `options.consent` is `{ kinds, tokens }`. For each kind in `kinds` the core needs a token of a
  live challenge with that kind, the same package id, the same version (`options.version`; none
  for an uninstall) and the same winget source of the command. Otherwise the command returns
  `invalid_config` and nothing starts. The kind `package` needs a version and is refused for an
  uninstall; no consent is valid for an update.
- The IPC layer never builds the consent that the executor sees. `services/install.rs`
  replaces `InstallRequest.consent` with the result of the check (`VerifiedConsent`), so a
  caller of the service cannot bring its own.
- Challenges end with the attempt: any operation of the package that ends in something other
  than `agreements_required`, any start with no consent (a new attempt), or 15 minutes. A token
  is not single-use inside the attempt, because the retry of the second kind carries both.
- At the argument level, `guard_args` (every call with no consent path) refuses both agreement
  flags. `guard_args_consented` lets a flag pass only for a `VerifiedConsent` that carries its
  kind, on `install` (both kinds) or `uninstall` (source only), as a bare argument, once.
  `--force` and `--ignore-security-hash` never pass.
- `open_agreement_link` opens a link only when a live challenge of that package showed it.
- Not covered, as before: a script that runs in the webview can start an operation, read the
  error, and send the consent itself. Only the rules of "Trust boundary" and the later native
  dialog (alternatives table) close that.

#### Alternatives

| Option | Status | Reason |
|---|---|---|
| Let winget show its own prompt in a visible console window. | Rejected | The app needs a console window for each operation, the progress parser gets no output in that mode, and a headless mode cannot use it. |
| Store the consent for each winget source. | Rejected | A stored consent accepts changed terms with no user action (round 1 finding K1 in `docs/reviews/2026-11-11-spec-review-2/`). |
| A config value that sets the flags for each operation. | Rejected | That is silent acceptance. Hard rule 4 forbids it. |
| Read agreements through the winget COM API. | Open | It depends on the executor decision of §2.2. |
| The core opens a native consent dialog, so that the webview carries no consent. | Open, not in WP5 | It removes the trust in the webview. Nobody checked if a native dialog can show a long agreement text. WP5 builds the webview dialog with the trust-boundary rules; this option is a later hardening step. |
| Ask winget, before the operation starts, if an agreement is necessary, and show the dialog first. | Open | It removes the failed first operation. No documented winget call gives that answer; spike question 1 shows if `winget show` can. Until then the flow starts the operation and reads the exit code. |

#### Spike (the first WP5 task)

Questions 1 and 3 gate the work on this section. Answer them first, and record the answers in
the `winget-executor` skill. Question 2 does not gate the work: the flow below is correct for
each exit code. Record its answer at the first real-winget run in CI (F2, #21).

1. Where does the agreement text come from when winget runs with no interactive prompt? The
   output of `winget show` is the first candidate. `ProgressParser::agreements_shown()` in
   `executor/winget_parse.rs` detects license text in winget output and has no caller; start
   there.
2. Which exit code does winget give for an install with a `--version` that does not exist?
3. Which agreement flags does `winget uninstall` accept?

The spike runs read-only winget commands first (`winget show`, `winget search`,
`winget source list`, `winget uninstall --help`) on a developer machine. If they cannot
answer question 1, the person who does the spike runs the install command by hand in Windows
Sandbox or in a manual CI job, never on the developer machine itself (§8). The spike is a
one-time investigation, not a required test, so the rule "no required manual step" of §8 does
not apply to it. The spike notes record where each command ran.

The answer to question 3 selects the uninstall work:

- `winget uninstall` accepts the source agreement flag: `start_uninstall` gets
  `options.consent` (§4.2), and `PackageExecutor::remove` gets a request.
- It accepts no agreement flag: `start_uninstall` gets no `options`, and `remove` keeps its
  signature. An uninstall that ends with `agreements_required` shows as an error.

Two more facts are useful for the skill and do not gate the work, because the rules above are
correct for each answer: does winget keep a source acceptance after one accepted call, and
does winget ask again when the terms of a winget source change? The answers decide only how
frequently the dialog opens.

The answer to question 1 decides between case (a) and not (a). The maintainer decides between
(b) and (c):

| Result | Core | Dialog | Tests that apply |
|---|---|---|---|
| (a) winget gives the agreement text. | Builds the `consent` option. Fills `agreement_text`. | The webview dialog shows the text. | The tests with the tag (a, b, c) or (a, b). |
| (b) winget gives no text, and the maintainer approved the link fallback. | Builds the `consent` option. Fills `agreement_url`. | The webview dialog names the package, the winget source and the kind of agreement, and shows the link of `agreement_url`. The link opens in the browser. | The tests with the tag (a, b, c) or (a, b). |
| (c) winget gives no text, and the fallback has no approval. | Builds no `consent` option; the *planned* mark stays in §5.4. The app never passes a flag. | No dialog. | The tests with the tag (a, b, c). |

**The "no text" rule, for each case.** An operation can end with `agreements_required` and
give the UI nothing to show: no `agreement_text`, and no `agreement_url` or no approval for
the link. The operation then ends as in step 2 of the flow (rest state, Info row). The UI
opens no dialog and shows this message, which it makes from `error.code` and the empty
fields: "The agreement text is not available. Store cannot install this package." In case
(c), each `agreements_required` ends so.

**Spike result and decision (2026-11-13).** The spike (`docs/spikes/2026-11-13-agreement-consent.md`) points to not (a): each
agreement that it found has a label and a link, and no text. The maintainer approved the
fallback of case (b) on 2026-11-12: a dialog that names the agreement and shows its link
satisfies hard rule 4. On 2026-11-13 the maintainer decided that **WP5 builds case (c)**, and
that case (b) follows as later work (backlog F4). In WP5 each `agreements_required` thus ended
with the "no text" rule, and the app passed no agreement flag. *Case (b) is built (F4,
2026-11-25):* `start_install` and `start_uninstall` take `options.consent`, an uninstall with the
source kind only (spike question 3); the "no text" rule still applies to an
`agreements_required` with no text, no link or no token.

#### Flow

1. The first operation of an attempt starts with no agreement flag.
2. When winget needs an agreement, the operation ends with the error `agreements_required`.
   One field of the error says which kind is missing, and the error carries the data for the
   dialog (§1.3). The ledger goes to the rest state (§6.7, the "winget needs an agreement"
   row).
3. The UI opens the consent dialog and shows no error. The dialog takes its data from the
   error: it names the package, the `version` and the `winget_source`, says which kind of
   agreement is necessary, and shows the `agreement_text` (case a) or the link of
   `agreement_url` (case b). When the "no text" rule above applies, the UI opens no dialog
   and the attempt ends.
4. **Accept.** The UI calls the start command again with `options.consent` (each kind that
   the user accepted in this attempt) and with `options.version` set to the `version` of the
   error. This call starts a new operation in the same attempt. The core passes the flag of each accepted
   kind for this one operation and installs exactly that version. The version rule is
   necessary: an agreement belongs to one package version, so a newer version can have terms
   that the user did not see. If that version is not available, the operation ends with the
   error that the exit code map gives; `package_not_found` is expected and not verified (spike
   question 2). The message names the version. The attempt ends there; the user starts a new
   attempt with no consent.
5. When winget then needs the second kind, steps 2 to 4 repeat, and the retry carries the two
   kinds. The dialog opens two times at most for one attempt, so an attempt has three
   operations at most.
6. **Decline.** The UI closes the dialog. Nothing more runs, and the attempt ends. The core
   gets no call and writes no log row; the `agreements_required` row of step 2 is the record.

On the branch, the UI marks an operation as failed when the terminal `store:progress` event
arrives, before `store:operation-finished`. *Planned (WP5):* the UI waits for
`store:operation-finished` and reads `error.code` before it shows a failure, so that
`agreements_required` opens the dialog and shows no error. The two handlers are in
`ui/src/lib/operations.svelte.ts`.

#### Read-only calls

WP5 builds no consent for a read-only winget call. A read-only call never passes an agreement
flag. When it ends with `agreements_required`, the command returns that error and the UI
shows it as an error.

The consent step for read-only calls belongs to the first work package that needs it: WP6
(`check_updates`, `update_all`, the library list), or the ADR 0001 work if its option A makes
search and detail read-only winget calls. That work package must define the step before it
writes code. The rules of this section apply to it: no stored consent, and a flag for one
call only.

#### Changes that WP5 owns

- Remove `accept_source_agreements` from `AppConfig`, the config type in `config.rs`. On the
  branch on 2026-11-11 it sets the source and package agreement flags for each operation,
  which is the silent acceptance that rule 4 forbids. It defaults to false, and no screen sets
  it, so no user loses a setting. *Built on the branch (2026-11-13).*
- Add `options.consent` to `start_install` (§5.4, §4.2), and to `start_uninstall` if spike
  question 3 permits it. In case (c) of the spike table, add neither. *Built, F4 (case (b)).*
- Add the dialog data to the `agreements_required` error (§1.3) and `license_url` to
  `PackageDetail` (§4.2). *The error data is built on the branch. `license_url`, `agreement_text`, `agreement_url`,
  `agreements` and `consent_token` are built in F4.*
- Add the "winget needs an agreement" row of §6.7 to the code. *Built on the branch.*
- Set the content security policy and add the text-only test of "Trust boundary". *The policy
  and its test are built on the branch. The text-only UI test is built (step 3).*
- Add the consent dialog. The UI must read `error.code`; on the branch it reads only
  `cancelled`. *Handler order built (step 3). The dialog is built in F4 (case (b)).*
- Update the `winget-executor` skill (hard rules, command lines, test list, state notes) to
  this section, in the same pull request. *Done on the branch (2026-11-13).*

Files that the change touches. Paths are relative to `src-tauri/src/` unless they start with
`ui/`, `src-tauri/` or `.claude/`. Status on 2026-11-13: each row is built on the branch,
except where its last words say otherwise:

| File | Change |
|---|---|
| `config.rs` | Remove the field and the test `default_has_agreements_off`. The test `modified_round_trips` drops the field and stays. |
| `ipc/state.rs` | Stop the copy of the config value into the executor options. |
| `executor/mod.rs` | `InstallRequest` carries the consent. If spike question 3 permits it, `PackageExecutor::remove` gets a request, so that a consent can reach it. `PackageExecutor::update` does not change. *Built, F4 (`RemoveRequest`, `VerifiedConsent`).* |
| `executor/winget_cli.rs` | Remove `WingetOptions.accept_agreements`. Six argument builders read it today. After the change the install builder takes the flags from the request, and so does the remove builder if spike question 3 permits it. The builders with no consent in WP5 (`list_installed_args`, `list_updates_args`, `pin_args`, `update_args`) pass no flag. `finish()` writes an Info row, not an Error row, for `agreements_required`, and fills the dialog data of the error (§1.3). The function that stops the tree becomes a field of the executor, and the "cancel failed" event is added (§6.7 gap table). In case (c) no builder takes a consent. |
| `services/install.rs`, `services/install/tests.rs` | `settle()` gets a case for `AgreementsRequired` before the general failure case, and the planned cancel rule for an uninstall. Update the table in the module comment. If spike question 3 permits it, `start_uninstall` takes the request and passes it to `remove`, and its calls in the tests change. *Built, F4; the check is in `services/consent.rs`.* |
| `error.rs` | Serialize the two agreement fields and the dialog data (§1.3). |
| `domain/package.rs`, `domain/progress.rs` | `license_url` on `PackageDetail` (*built, F4*). `cancel_failed` on `ProgressEvent`. |
| `ipc/commands.rs`, `ipc/test_support.rs` | The `consent` option on the commands. `FakeExec` implements the changed trait and records its requests. *Built, F4; also `open_agreement_link`.* |
| `src-tauri/tauri.conf.json` | The content security policy. |
| `ui/src/lib/api.ts`, `types.ts`, `operations.svelte.ts` | The option, the error fields, `license_url`, `cancel_failed`, the handler order. *Step 3; the option, `license_url` and `openAgreementLink` are built in F4.* |
| `ui/src/components/ConsentDialog.svelte` | New. *Built, F4.* |
| `src-tauri/tests/fixtures/fake_winget.rs` | Only if a new case needs it. The fake returns the two agreement exit codes today. |

An update that needs an agreement ends with `agreements_required` in WP5, and the UI shows it
as an error. WP6 defines consent for an update (§7).

Tests that asserted the config rule on 2026-11-11. Each one is rewritten on the branch to
assertions with no agreement flag in any case (`install_passes_no_agreement_flag_in_any_case`,
`no_command_passes_an_agreement_flag`, `no_builder_passes_an_agreement_flag`); the smoke test
passes no flag and no longer reads `STORE_SMOKE_ACCEPT`. The list names them as they were:

- `executor/winget_cli.rs`: `agreement_flags_only_when_accepted`,
  `pin_agreements_flag_only_when_accepted`.
- `tests/executor_winget.rs`: `agreements_flags_follow_config`,
  `agreement_flags_on_update_remove_list_and_pin`,
  `list_updates_parses_table_and_skips_unmatched_rows`.
- `tests/winget_smoke.rs`: `install_then_remove_one_small_package`.

Edits that were not a rewrite: the fixture `Fx::new` in `tests/install_loop.rs` set the removed
option, and the helper `Fx::accepting` in `tests/executor_winget.rs` is removed.

Tests to add. The tests for the two agreement exit codes exist in
`tests/executor_winget.rs` and in `executor/winget_parse.rs`; reuse them and add tests at the
ledger and service level. The tag after each test names the cases of the spike table in which
it applies. Status on 2026-11-13: the (a, b, c) tests outside `ui/` are built on the branch
(`agreements_required_carries_dialog_data_and_logs_info`,
`agreements_required_on_remove_and_update_has_source_and_no_version`,
`cancel_that_cannot_stop_the_tree_emits_one_cancel_failed_event`, `csp_policy.rs`); the (a, b)
tests are built in F4 (list below; each names its test); the UI tests are step 3 and F4:

- (a, b) An accepted retry passes only the accepted flags and `options.version`. This test
  runs against the subprocess fake (`tests/install_loop.rs`, `tests/executor_winget.rs`),
  because `FakeExec` ignores its request. *Built, F4:* `consented_operation_alone_passes_its_flags`
  and `a_package_only_consent_gives_uninstall_no_flag` (`tests/executor_winget.rs`);
  `accepted_retry_passes_only_the_accepted_kind_and_the_version`
  (`services/install/tests.rs`; `FakeExec` now records its requests).
- (a, b, c) A first `agreements_required` leaves the ledger in the rest state, writes an Info
  row, and its JSON carries the fields of §1.3.
- (a, b) A version that is gone ends the attempt with the error that the exit code map gives.
  *Built, F4:* `consented_retry_with_a_gone_version_ends_with_the_mapped_error`,
  `a_failed_retry_ends_the_attempt_and_its_tokens`.
- (a, b, c) No operation passes a flag with no consent. A call with no consent path (list,
  pin, update) never passes a flag. *Rewritten, F4:* `no_command_passes_an_agreement_flag`,
  `no_builder_passes_an_agreement_flag_without_consent`, `guard_rejects_forbidden_flags`,
  `consented_guard_lets_through_only_the_consented_flag_on_install_and_uninstall`.
- (a, b) `start_install` with the kind `"package"` and no `options.version` returns
  `invalid_config`. *Built, F4:* `package_kind_needs_a_version_and_unknown_kinds_fail`
  (`services/consent.rs`).
- (a, b) `start_uninstall` with the kind `"package"` returns `invalid_config`. *Built, F4:*
  `uninstall_consent_is_source_only_and_bound`.
- (a, b) A consent for another package, version, winget source or kind, or with no token, starts
  nothing. *Built, F4:* `consent_is_bound_to_package_version_source_and_kind`,
  `no_token_or_a_made_up_token_is_refused` (`services/consent.rs`);
  `accepted_retry_passes_only_the_accepted_kind_and_the_version`;
  `a_link_that_no_challenge_showed_does_not_open` (`ipc/tests.rs`).
- (a, b, c) `tauri.conf.json` holds `script-src 'self'`. The test is the new file
  `src-tauri/tests/csp_policy.rs`.
- UI tests (see the WP5 entry in §7 for when they can exist; the (a, b) tests are built in F4,
  `ui/src/components/ConsentDialog.test.ts`, each named like its line below):
  - (a, b, c) catalog text with markup appears as text;
  - (a, b, c) `agreements_required` with nothing to show gives the "no text" message and no
    dialog;
  - (a, b) `agreements_required` with a text or an approved link opens the dialog and shows
    no error;
  - (a, b) "Decline" makes no call; Accept sends the consent bound to the package and the
    version of the error; the link opens only through `open_agreement_link`; the agreement
    shows as text;
  - (a, b, c) an event with `cancel_failed` disables Cancel.

### 6.7 Install state transitions (WP5, built on branch)

The terms *ledger*, *operation*, *active*, *rest state* and *busy state* are in §0.

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

`target_version` is cleared in each row. `services/install.rs` settles the ledger. The
executor (`executor/winget_cli.rs`) writes the `operation_log` row of the operation.

| Operation | Outcome | Terminal stage | Ledger state | Log row | Error code |
|---|---|---|---|---|---|
| Install, update | Success | `done` | `Installed`; version = target | Info | none |
| Install, update | Failure, a version was installed before | `failed` | `Installed`; version kept | Error | from the exit code |
| Install | Failure, no version before | `failed` | `Failed` | Error | from the exit code |
| Install, update | Cancel, a version was installed before | `cancelled` | `Installed`; version kept | Info | `cancelled` |
| Install | Cancel, no version before | `cancelled` | `NotInstalled` | Info | `cancelled` |
| Uninstall | Success | `done` | `NotInstalled`; version and pin cleared | Info | none |
| Uninstall | Failure | `failed` | `Failed`; version kept | Error | from the exit code |
| Uninstall | Cancel | `cancelled` | The rest state | Info | `cancelled` |
| Any | winget needs an agreement | `failed` | The rest state | Info | `agreements_required` |

- A row that has the pin flag goes back to `Pinned` where the table says `Installed`.
- The exit code map is in the `winget-executor` skill.
- The rest state does not keep an earlier `Failed`: a package that was `Failed` with no
  version is `NotInstalled` after an agreement stop or a cancelled uninstall. Each edge that
  the rest state needs is legal today.
- A cancelled uninstall goes to the rest state. An uninstall can start from `Failed` with no
  version; the cancel then gives `NotInstalled`, not `Installed`.
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
  real outcome settles the ledger. The executor writes a Warn row to `operation_log`. It also
  emits one `store:progress` event with the stage unchanged, `cancel_failed` = true and the
  message "cancel failed: the installer continues". The send waits for room for 2 s at most
  and never uses the slot of the terminal event; after 2 s it writes a Warn row. The UI
  disables Cancel for that operation (step 3). A second cancel for the same operation does
  nothing.

#### Restart

The core is built by the first IPC command that needs it after the app starts. At that time,
each ledger row in `Installing` becomes `Installed` when a version is recorded and `Failed`
when not. Each ledger row in `Uninstalling` becomes `Failed`. `services/install.rs` writes one
Warn row to `operation_log` for each changed ledger row. It does not look for a winget process that survived the last run.

#### Known gaps

The review ids are from `docs/reviews/2026-11-11-spec-review.md`.

| Review id | Gap | Planned fix | Owner |
|---|---|---|---|
| none | A cancel that cannot stop the tree is not reported to the UI. The Cancel button stays and does nothing. | *Fixed on the branch (2026-11-13); the UI part, to disable Cancel, is step 3.* The condition: the tree is still alive after the Job Object stop and after `taskkill` (the site of the Warn row in `executor/winget_cli.rs`). The executor then emits one `store:progress` event with the stage unchanged, `cancel_failed` = true (§4.2) and the message "cancel failed: the installer continues". The progress call of today can drop an event. This send waits for room in the channel, for 2 s at most; after 2 s the executor writes a Warn row and continues. The send must not use the slot that is kept for the terminal event. The UI disables Cancel for that operation when `cancel_failed` is true; it does not match the message text. Test seam: the function that stops the tree becomes a field of the executor, with the real `kill_tree` as the default. Test: a stop function that reports a live tree. | WP5 |
| A2 | No agreement consent. | §6.6. *Case (c) is built on the branch (2026-11-13); case (b) is F4.* | WP5 |
| none | `check_updates` names no winget source, so it queries each winget source that is configured. | Name the `winget` winget source (§6.3). *Built on branch (WP11 step 4): `list_updates_args` passes `--source winget`.* | WP6 |
| none | A cancelled uninstall records `Installed` when no version is recorded. | The cancel goes to the rest state. *Fixed on the branch (2026-11-13).* | WP5 |
| A24 | "An operation is active" returns `unsupported`, and the UI cannot separate it from other refusals. | Add the code `busy`, retryable. | WP6 |
| none | The recorded version is the catalog target, not the version that winget installed. | *Built (2026-11-25, catalog plan step 7).* After a successful install or update, `services/install.rs` reads the version from `winget list` (`list_installed`) and records it. When the list call fails or has no row for the package, the catalog target stays and one Warn row says why. A difference between the two writes a Warn row. A listed version of `Unknown` or a range (`<`, `>`) counts as no row. The call is narrowed to the package: `list_installed_one` runs `winget list --id <id> --exact --disable-interactivity` with a 30 s limit (`LIST_ONE_TIMEOUT`). The full `list_installed` (Library) keeps no filter and has a 120 s limit (`LIST_TIMEOUT`). | The F1 fix |
| A10 | After a failed uninstall and after a restart, the ledger guesses. | Check the installed list and set `Installed` or `NotInstalled` from it. Keep `Failed` for "install tried, no version". | WP6 |
| A10 | Nothing writes `Pinned` or `UpdateAvailable`; no pin command exists. | Pin becomes a flag; update-available comes from the scan. Rename the struct `UpdateAvailable`, which has the name of a state. | WP6 |
| A11 | Two installs of different packages can run together. | One queue for each operation that changes the system. Installer exit 1618 maps to a retryable busy error. | WP6 |
| none | A second app instance has a separate lock on the same ledger. | One app instance only. | WP6 |
| A13 | The exit code is not stored in `operation_log`. The "nothing to do" exit code counts as success for install, update and uninstall. | An `exit_code` column; a rule per operation. | WP6 |
| A24 | A paid item, an illegal transition, a timeout, a rate limit and a busy installer have no code of their own. | New codes. | WP6 |
| none | When `services/install.rs` cannot settle the ledger, the failure goes to the trace log only. | Write an Error row to `operation_log`. | WP6 |
| A12 | Cancel does not depend on the stage; an elevated installer cannot be cancelled. | The elevated part is closed by the `cancel_failed` path (first row of this table). The stage-dependent cancel moves to the WP6 queue work. | WP6 |
| A24 | The UI never sets the install scope, so winget uses its default. | A scope choice and the elevation path. *Elevation core built (step 1); the scope choice is step 2.* | WP11 |

### 6.8 Elevation (WP11, step 1 built)

Design: `docs/designs/hardening.md` section 1.

1. The app runs as the user who started it (manifest level `asInvoker`). It never asks for
   elevation for itself.
2. winget runs as a child with the same token. An installer that needs administrator rights
   asks Windows, and Windows shows its own UAC prompt. The app shows no prompt of its own.
3. `start_install` takes `scope` (`user`, `machine` or none); none passes no `--scope` flag.
   Uninstall and update pass no scope.
4. Exit codes `0x8A150019` and `740` give `ElevationRequired` (`executor/winget_parse.rs`).
   The terminal stage is `failed` and the ledger follows the "Failure" rows of §6.7. The code
   of a refused UAC prompt is not verified; until a fixture proves it, a refused prompt ends
   as `winget_failed`.
5. `platform/windows.rs` holds the trait `ElevationCheck` and `WindowsElevation`, which reads
   `TokenElevation` of the process token. A failed read counts as not elevated. The command
   `app_status` returns `{ elevated: bool }`. When `elevated` is true, the UI (step 2) shows:
   "Store runs as administrator. Installers will not ask before they change the system." The
   banner only informs; the app does not refuse to run.

### 6.9 Toasts (WP11, step 3 built)

Design: `docs/designs/hardening.md` section 2. Built with the `windows` crate (Â§2), features
`UI_Notifications` and `Data_Xml_Dom`; no new crate and no webview permission.

1. `platform/windows.rs` holds `trait Notifier { fn show(&self, &Toast) -> Result<(), String> }`
   and `trait WindowFocus { fn is_focused(&self) -> bool }`. The real parts are `WindowsNotifier`
   (`ToastNotificationManager::CreateToastNotifierWithId`; the AppUserModelID is the bundle
   identifier read from the Tauri config) and `TauriWindowFocus` (`WebviewWindow::is_focused`
   of the window `main`; a missing window counts as not focused). Tests use `FakeNotifier` and
   `FakeWindowFocus`. `Core.toasts` holds the two parts; `AppState::with_toasts` sets them.
2. `toast_xml` builds two `<text>` lines and nothing else: no image, link or action element.
   All text is escaped as XML text.
3. A click runs an in-process `Activated` handler that shows, restores and focuses the main
   window. Activation after the app has closed is not handled.
4. `services/notify.rs`, called in `install::drive` just before the finished event: an
   operation (install, update, uninstall) that ends while the window has no focus gives one toast, "Install finished" /
   "Foo.Bar was installed.", "Install failed" / "Foo.Bar did not finish. Open Store for
   details.", or, for `agreements_required`, "Open Store to review an agreement." A cancel
   gives none (the user did it). No toast when `notifications_enabled` is the text `"false"`;
   any other value, or none, counts as on. The checks run in the operation; the call into
   Windows runs on a blocking thread in a background task (`notify::show`), so `update_all`
   never waits for a toast. A failure or a panic of that call does not change the result of
   the operation or its finished event (`ok` stays as the operation ended).
5. A failed toast call writes one Warn row (message "toast failed: ...") to the operation log
   (`operation_log`) and nothing else. A shown toast writes one Info line to the application
   log (tracing), not to the operation log. No network call, no other record.
6. The toast of a scheduled scan is §6.10. Not built: a settings command or UI for
   `notifications_enabled` (the key is read; nothing writes it yet).
7. Both Tauri installers set the AppUserModelID on the Start menu shortcut: NSIS
   (`SetLnkAppUserModelId` in `nsis/includes/shortcuts.nsh`) and MSI (the WiX template
   `bundle/windows/msi/main.wxs` sets `System.AppUserModel.ID`). Read in tauri-bundler, not
   tested on an installed build. A development build, or any build with no registered
   AppUserModelID, most likely gets a silent `Ok` from `Show`: no toast and no Warn row.

### 6.10 Scheduled update scan (WP11, step 4 built: the in-app timer)

Design: `docs/designs/hardening.md` section 3. Option (a) only: a timer in the app. The Task
Scheduler option (b) is an open owner question and is not built.

1. `services/schedule.rs`: `schedule::run(core, clock)` starts at app start-up (`lib.rs`) once
   the core exists. It ticks at once and then every 15 minutes (`TICK`) and calls
   `scan_if_due(core, now)`.
2. Settings keys (§4.3): `update_scan` is `off` (default; also for a missing or unknown value),
   `daily` or `weekly`. `update_scan_last` is the Unix time of the last scan attempt.
   `update_scan_last_ids` is the sorted list of package ids of the last scan toast, joined
   with `,`. A scan is due when there is no last time, the last time is in the future, or the
   last time is at least one interval old (24 hours, 7 days).
3. A scan does not run while `ActiveOperations` has an entry (`Busy`); it is due again at the
   next tick. (The wait for the WP6 queue comes with WP6.) The check does not cover the gaps
   between two packages of an `update_all` run; that is safe, because the scan only reads.
4. A scan calls `list_updates()` only, which is `winget upgrade --include-unknown --source
   winget --disable-interactivity` (`list_updates_args`; no package, no `--all`, no agreement
   flag). It never starts an install, an update or a removal. The attempt time is saved after
   every call, so an error waits for the next interval.
5. Result:
   - Updates, and the id set differs from `update_scan_last_ids`: one toast, "Updates
     available" / "N updates are available." ("1 update is available."), only when
     `notifications_enabled` is not `"false"`. The ids are saved only after the toast was
     shown. The window focus is not asked.
   - Updates, same set: no toast. No updates: no toast; the saved ids are cleared.
   - `agreements_required`: one toast, "Update check needs you" / "Open Store to review an
     agreement."; no flag is passed and no consent is stored (§6.6, "Read-only calls").
   - Any other error (`winget_not_found`, `source_unavailable`, a time-out): no toast, one Warn
     row "update scan failed: ..." in the operation log (no package, no operation).
6. A failed scan toast writes the Warn row of §6.9 rule 5; the scan result stays.
7. Commands: `get_update_scan` returns the mode text; `set_update_scan(mode)` saves it and
   refuses any text other than `off`, `daily` and `weekly` with `invalid_config`. The UI
   (`UpdateScanSetting.svelte`) is a select of the three modes.
8. Not built: a scan while the app is closed (the Task Scheduler option), a scan toast that
   opens the update page, and the wait for the WP6 queue.

---

## 7. Work packages (in this order)

WP6 does not wait for the F1 fix. The gap row "recorded version" of §6.7 belongs to the F1
fix, in whatever order the two land.

Status on 2026-11-13. The issue numbers are the GitHub issues and the entries of
`docs/ISSUE-BACKLOG.md`.

| WP | Issue | Status |
|---|---|---|
| WP0 | #1 | Built, on `main`. |
| WP1 | #2 | Built, on `main`. |
| WP2 | #3 | Built, on `main` (PR #14). The catalog service returns unrelated results: bug F1, #20. |
| WP3 | #4 | Built, on `main` (PR #15). |
| WP4 | #5 | Built, on `main` (PR #18). |
| WP5 | #6 | Built on branch `impl/6--install-loop` (PR #19, open). Merged 2026-11-25. The work that remains is split (maintainer decision of 2026-11-12). Steps 1 and 2 of the WP5 entry below are done (2026-11-13); PR #19 is merged. To close WP5: step 3, in a second pull request. The component-test level exists (F2, #32). |
| MVP milestone | #6, #20 | Reached when WP5 is closed and bug F1 (#20) is fixed. F1 is a catalog change with its own pull request (ADR 0001). |
| WP6 to WP11 | #7 to #12 | Not started, with two exceptions. WP6: `check_updates` and `update_all` exist as commands with no UI. WP7: the Buy-in-Store button and `open_store_page` are built on branch `impl/6--install-loop`. |

**WP0 — Scaffold & CI.** Create crate, Tauri 2 bootstrap, `error.rs`, `config.rs`, SQLite pool +
migrations, tracing. Add GitHub Actions: `windows-latest` build + `cargo test` + clippy + fmt.
*Accept:* a window opens, DB initializes, config round-trips.

**WP1 — Domain + DB.** Implement §4 enums/structs, migrations, repositories.
*Accept:* unit tests pass for row<->domain mapping and migration apply-from-empty.

**WP2 — Catalog source: community.** Implement `CatalogSource` trait + `winget_repo.rs` using a
public JSON catalog endpoint; parse into `PackageSummary`. Mock the endpoint in tests
(httpmock).
*Accept:* `search("firefox")` returns typed summaries in an integration test without winget.
*After the merge:* the endpoint returns stale rows (bug F1). ADR 0001 selects the backend that
replaces it; the F1 fix (#20) owns that change and its acceptance criteria.

**WP3 — Executor: winget CLI.** Implement `winget_cli.rs`: discover winget path, version check,
install/remove/update wrappers, stdout parser (progress + errors), cancel by a stop of the
process tree (Job Object, then `taskkill /T /F`); winget has no cancel call. Fake `winget`
executable (the cargo example `fake_winget`) for tests.
*Accept:* tests drive install/update/remove against the subprocess fake (§8) and parse every
stage. The test
with a real winget install follows §8: CI or a sandbox only.

**WP4 — IPC + UI skeleton.** Define the Tauri commands `app_version`, `search_sources`,
`get_app_detail`, `start_install`, `cancel_install`, `check_updates`, `update_all`,
`open_store_page` and `start_server`, and the events of §5.4; create frontend with search page
(debounced input -> results grid) and install button wiring.
*Accept:* a search from the UI shows results from the community catalog source; a click on
Install starts an install with live progress. Automated tests use the subprocess fake or `FakeExec` (§8).

**WP5 — Detail + full install loop.** Built on the branch: the detail page (description and
versions; screenshots and rating show only when the catalog source gives them, and the
community source gives none), cancel, uninstall, log tail, and the commands `start_uninstall`,
`get_install_status`, `get_operation_log` (§5.4). State rules: §6.7. Work that remains, in
this order:

1. ✅ The spike of §6.6, done on 2026-11-13 (`docs/spikes/2026-11-13-agreement-consent.md`). Result: not (a). The maintainer
   selected case (c) for WP5; case (b) is later work (backlog F4). In case (c),
   `start_uninstall` gets no `options`.
2. ✅ The core work, done on 2026-11-13: the parts of "Changes that WP5 owns" in §6.6 that
   are outside `ui/`, and the WP5 rows of the gap table in §6.7 (the cancel that cannot stop
   the tree, and the cancelled uninstall).
3. ✅ Built on branch `impl/6--ui-step3` (2026-11-25), second pull request. The UI work: the handler order, the "no text" message, the disabled Cancel button, and the
   UI tests of §6.6 with the tag (a, b, c). In case (c) there is no consent dialog. It is a second pull request.

PR #19 is merged (2026-11-25). Until step 3 merges, the UI shows `agreements_required` as an error with its message. In case (c), a package that needs an agreement cannot install through the app until the work of backlog F4 is done. The UI tests of §6.6 gate the second pull request.

*Accept:*

- Each row of the two transition tables in §6.7 has a test that passes against the
  subprocess fake or `FakeExec` (§8). A test of flags or exit codes uses the subprocess fake.
- The tests of §6.6, "Tests to add", pass: each test whose tag names the selected case.
- The "cancel failed" test of the §6.7 gap table passes.
- The text `accept_source_agreements` occurs nowhere in `src-tauri/` or in the
  `winget-executor` skill.
- The pull request holds the skill update.

WP5 and the F1 fix together are the MVP milestone.

**WP6 — Library + updates.** Installed list, update badge, update one/all with summary, the
pin model and the pin commands, and the WP6 rows of the gap table in §6.7. Before code, WP6
must define:

- the executor decision of §2.2 (the winget CLI or the COM API);
- the pin behavior: the gap table says pin becomes a flag; what a pin blocks is not decided;
- the conditions for "skipped" in `update_all`, and the set of packages in its summary;
- the queue for operations that change the system, and the one-instance rule;
- the consent step for read-only calls, for an update and for the pin commands (§6.6).

*Accept:* `check_updates` finds an outdated package in a test against the subprocess fake;
`update_all` ends with each package of the summary set marked updated, skipped or failed. The
pin criterion follows from the pin definition above.

**WP7 — msstore source + purchase hand-off.** `msstore.rs` adapter, through the winget CLI
only. The adapter sets `is_paid` by the rule of §6.4, and review action A17 defines how it
reads the price data. The Buy-in-Store button and `open_store_page` are built on branch
`impl/6--install-loop`; WP7 connects them to msstore items. Before code, WP7 must define: the
winget sources that a read-only call names (§6.3), the priority rule for a merged search card
(§5.3), and the consent step for the `msstore` winget source agreement.
*Accept:* with `msstore` on, a search in a test returns a free Store item as a
`PackageSummary` with `is_paid = false`; with `msstore` off, the adapter returns
`SourceUnavailable`; a paid item and an item with no price data show only the browser
hand-off (no install path).

**WP8 — Polish.** Categories/featured, ratings/screenshots caching, the image proxy decision
(§2.3), dark/light theme, empty/error/loading states, accessibility pass.
*Accept:* not defined. Before code, WP8 must write criteria that a test can decide (the
screens, the keyboard path on each one, the visual checklist), and the maintainer must
approve them.

**WP9 — Extra sources.** Deferred (§2.1). The first idea is Scoop and Chocolatey adapters
behind the same traits, and a source picker in the UI. Not designed.

**WP10 — Headless mode.** Blocked (§6.5). No design is accepted; a decision record comes
first and defines the acceptance criteria.

**WP11 — Hardening.** UAC elevation for the machine scope, toast notifications, background
scheduled update scan (an in-app timer, §6.10; Task Scheduler is an opt-in after an owner decision), winget import/export. An install of a local MSIX file
is deferred (§2.1, review action A9). No item has a flow in §6. Before code, WP11 must add a
flow for each item, with criteria that a test can decide. The import flow must show how one
user click for a list of packages satisfies hard rule 4.
*Accept:* not defined; see above.

### 6.11 Import and export (WP11; export built on branch, step 5; import read built on branch, step 6; import install planned, step 7)

Design: `docs/designs/hardening.md` §4. The file is winget's own JSON format (`Sources`, each
with `SourceDetails` and `Packages`), so plain winget reads a file that Store writes.

**Export** (*built on branch*). `export_packages`:

1. `WingetCli` runs `winget export -o <temp file> --source winget --disable-interactivity`
   (`export_args()`, checked by `guard_args()`). It passes no agreement flag, although winget
   accepts one on export. A source agreement ends the call with `agreements_required`; nothing
   is written.
2. The command lists the installed packages that the `winget` winget source knows, not only
   the packages that Store installed. It changes nothing on the system.
3. `services/export.rs` reads the temp file after a size check (`metadata().len()`, at most
   1 MiB, before the read) and checks it with the import parser (below). When it parses, the
   file goes to `Documents\Store\exports\store-export-<date>.json` (UTC date; a second export
   on the same day replaces the file): it is written to a temp file in the exports folder
   (name ends in `.tmp`) and renamed over the target, so no reader sees a half-written file.
   Otherwise the command returns `winget_failed` and keeps no file. The export temp file is
   removed on every path, also when winget wrote a file and then exited non-zero; a failed
   write removes the folder temp file.
4. No `--include-versions`: an import installs the latest version.
5. The UI ("Export installed apps" in the header) shows the file name, the count and the path
   as text, with an "Open folder" button (`open_exports_folder`) and a "Review this file"
   button that opens the file in the import review. No file dialog (design "Decisions" 9).
   `open_exports_folder` starts `%WINDIR%\explorer.exe` by full path. The agreement hint in
   the UI is plain text with no markup characters.

*Tests:* `export_args_have_no_agreement_flag` (`executor/winget_cli.rs`);
`tests/import_export.rs` (`outfile` directive of the fake: copy, bad file, no file, source
agreement; the temp file is removed on every path; winget writes a file then exits non-zero:
nothing kept; an oversized temp file; a second export replaces the file);
`ui/src/components/ExportPanel.test.ts`.

**Import: read** (*built on branch, step 6*; install is step 7, planned). `read_import_file`:

1. **Path.** *String check first:* `is_local_drive_path()` (a pure function, no disk access)
   refuses anything but an absolute local drive path (`C:\dir\file`): a UNC path, a device
   path (`\\.\`, `\\?\`, `\\?\UNC\`), a relative or drive-relative path, a second `:`, a NUL. `has_reserved_device_name()` (also pure) refuses a Windows reserved device name in any component (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `COM0-9`, `LPT0-9`, the superscript `¹²³` forms), case-insensitive, with or without an extension, trailing dots or spaces. `GetDriveTypeW` refuses a drive whose type is `DRIVE_REMOTE` (a mapped network drive; no path I/O; it is passed in as a function so tests use a fake). All three run
   before `canonicalize` (a string equal to a recorded drop path, which is canonical and may start with `\\?\`, is matched first, with no disk call), because resolving a network or pipe path connects out (NTLM hash
   leak). Then the core opens a path only in two cases. *Dropped:* the main window's
   `DragDropEvent::Drop` is handled in Rust (`lib.rs`); each dropped path is canonicalised and,
   when it is a regular file, recorded in `ImportPaths` (`services/import.rs`; in `AppState`;
   at most 16 entries, the oldest goes first, in memory only). The webview gets the canonical
   path and the file name in `store:import-dropped`. *Exports folder:* the canonical path of a
   regular file with the extension `.json` whose parent is the canonical exports folder. Any
   other path, a missing path, a directory, or a link that resolves outside the folder gives
   `invalid_config`. No file-dialog dependency. *Open:* the file is opened once, with
   `FILE_FLAG_OPEN_REPARSE_POINT`, and the handle is checked (not a reparse point, a regular
   file); size and bytes are read from that handle. Remaining gap: a link swapped into a
   parent folder between the path check and the open is not caught.
2. **Parser.** `parse_import()` reads UTF-8 (a BOM is allowed) into a strict subset of the
   schema: `Sources[].SourceDetails.Name` (optional) and `Sources[].Packages[]` with
   `PackageIdentifier` (non-empty) and `Version` (optional); other fields are ignored. Limits:
   1 MiB (`metadata().len()` is checked before the read, and the read is capped) and 500
   packages (counted before duplicates are merged). Larger, bad JSON or an empty id gives
   `invalid_config`. The export check uses the same parser, so an export of more than 500
   winget packages (or over 1 MiB) fails with `winget_failed` and saves nothing; the UI says
   that Store exports at most 500 apps. Ids with Unicode format characters (category Cf, such
   as `U+200B` to `U+200F`, `U+202A` to `U+202E`, `U+2060` to `U+2064`, `U+FEFF`) or
   `U+FFFE`/`U+FFFF` fail the id check.
3. **Id check.** `validate_winget_id()` (`domain/package.rs`) follows the `PackageIdentifier`
   pattern of the winget manifest schema (read from manifest version 1.10.0, the latest on
   `microsoft/winget-cli` master, 2026-11-25; 1.9.0 has the same line) plus the leading-`-`
   rule and `U+007F`. The rules are in the design, §4, "Import: the file", rule 3.
4. **Rows.** Duplicates (same id and source, case-insensitive) merge into the first row. Each
   row gets one status, checked in this order: `source_not_supported` (source name is not
   `winget`, or the file has none), `not_valid` (the id fails the check), `installed` (the
   ledger has the id in state `installed`, or `winget list` has it; compared case-insensitive),
   else `ready`. Only `ready` can be ticked. Winget-source packages are never paid, so the
   paid rule has no row. When `winget list` fails, no row is `installed` from it and
   `installed_known` is false; the UI says so. `Version` is shown, never used.
5. **UI.** `ImportPanel.svelte`: a drop hint, then the file name and one row per package with
   its own tick box (ticks keyed by `package_id|source`; none ticked at the start, no "select all": open owner question, design
   "Owner questions" 3). It shows "N selected". It has no install button: install is step 7.
   All file text is shown as text.
6. **Effects.** The command changes nothing on the system. It starts one process at most:
   `winget list`.

*Tests:* `domain/package.rs` (id accept and refuse lists); `services/import.rs` (parser, limits,
review statuses, dropped path, exports folder rules, `..`, link, 17th drop, directory, size
check); `ipc/tests.rs` (`read_import_file_reads_a_dropped_file_and_refuses_others`);
`ui/src/components/ImportPanel.test.ts`.

---

## 8. Testing & QA strategy

- **Unit:** parsers (winget output, catalog JSON), state transitions, dedupe/sort, config.
- **Integration (HTTP):** `httpmock`/`wiremock` serving canned catalog JSON; assert parsing and
  error handling for malformed/rate-limited responses.
- **Subprocess:** the *subprocess fake* is the cargo example `fake_winget`
  (`src-tauri/tests/fixtures/fake_winget.rs`), a binary that prints scripted stdout and exit
  codes. It exercises the executor and progress parser deterministically.
- **In-process:** `FakeExec` (`ipc/test_support.rs`) implements `PackageExecutor` with no
  subprocess, for service and command tests. It ignores its request, so it cannot test flags.
  This document does not use "the fake winget" for the two.
- **Real winget:** only on a GitHub-hosted runner. The guard in `tests/winget_smoke.rs`
  refuses each other machine before any winget process starts, and `#[ignore]` keeps it out of
  `cargo test`. See `docs/designs/unattended-testing.md`. No other test file may call
  `WingetCli::connect` (checked by `tests/no_real_winget.rs`).
- **Required tests:** L1 (Rust) and L2 (UI: Vitest, `jsdom`, `mockIPC`), through
  `scripts/test.ps1`. The script writes `src-tauri/target/test-report.md`. No required manual
  step. The shared IPC fixtures are in `src-tauri/tests/fixtures/ipc/`.
- **Open checks:** the table in "Hand-over from SPEC §8" of
  `docs/designs/unattended-testing.md` (elevation prompt, cancel during a real download,
  offline UI state, no network call outside hard rule 5).

---

## 9. Guardrail & review checklist (run before every merge)

Each item names its check. "From WP5" means: after the WP5 work that remains is merged.

1. `is_paid` items have no install path in-app; only the browser hand-off. Check: the test
   that `start_install` refuses a paid item.
2. No account, sign-in, token or password handling introduced anywhere. Check: review of the
   diff.
3. Catalog sources call only the destinations that hard rule 5 permits and the endpoint that
   §0 names. Each adapter sets a timeout and a retry backoff as constants. Check: review of
   new URLs in the diff, and the adapter test in which a slow endpoint gives a timeout error.
4. winget source agreements and package agreements are shown to the user and never accepted
   silently. Check, from WP5: the §6.6 test "no operation passes a flag with no consent".
5. No Microsoft trademark/logo assets bundled; original product identity. Check: review of
   new asset files.
6. Each operation (§0) has exactly one outcome row in `operation_log`, with a timestamp and
   the outcome of the §6.7 end table. More Warn and Error rows can exist. A consent retry is
   a new operation and has its own outcome row. From WP6 the row also holds the exit code
   (§6.7 gap table, A13). Check: the `operation_log` assertions in `tests/install_loop.rs`.
7. From WP5: no setting, database column or file stores a consent (§6.6). Check: review of
   the diff for new settings and columns.
8. From WP5: the content security policy holds `script-src 'self'`, and catalog data appears
   as text only. Check: the two tests of §6.6.