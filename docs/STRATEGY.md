# Rebuilding the Windows App Store — Open Source in Rust

> **Goal:** an independent, open-source replacement for the Microsoft Store client on Windows,
> written in Rust.
>
> **Compatibility note (scope boundary):** we build a *compatible alternative* using public and
> community interfaces (winget, MSIX, public catalog endpoints). We do **not** extract or
> re-ship Microsoft's proprietary client code, and we must not use Microsoft trademarks in the
> product name.
>
> **Purchases are out of scope — deferred to a browser popup.** When a paid/entitled app is
> encountered, show a **"Buy in Microsoft Store"** button that opens the official product page in
> the default browser: web `https://apps.microsoft.com/detail/<productId>` or the Store deep link
> `ms-windows-store://pdp/?ProductId=<productId>`. We never handle payment data, accounts, DRM,
> or licenses ourselves.

---

## 1. Triage

### 1.1 What the Microsoft Store client actually is

The Store app is a thin UI over several backend services:

| Concern | Backend | Rebuildable in OSS? |
|---|---|---|
| Browse / search / catalog metadata | Store catalog service (semi-public REST) + winget community manifests | ✅ Yes |
| Download & install app packages | MSIX/MSIXBUNDLE/AppX via CDN + `Add-AppxPackage` / Deployment API | ✅ Yes |
| List installed / uninstall / update apps | PackageManager (Windows.Management.Deployment) | ✅ Yes |
| Free-app licensing (no payment) | license/activatnio tokens issued per package | ⚠️ Mostly, via winget msstore source |
| Downloads of classic Win32 apps | winget manifests (installer URLs) | ✅ Yes |
| Purchases / payment / account entitlements | Microsoft account commerce | ❌ No (proprietary) |
| DRM'd content (movies, games, in-app) | Microsoft DRM services | ❌ No |

**Verdict:** *80–90% of what people use the Store for* (find, install, update, remove free apps)
is rebuildable with public interfaces. The remaining 10–20% (paid/entitled/DRM content) is not,
and should be explicitly descoped.

### 1.2 Feature triage (MoSCoW)

**Must (v1)**
- Search across sources
- App detail page (description, version, publisher, size, screenshots)
- Install / uninstall / cancel
- Live progress + logging
- Update available / update all

**Should (v2)**
- Library of installed apps
- Category browsing & "featured" carousel
- Multiple sources: winget community repo + msstore + Scoop + Chocolatey
- Dark/light theme, Fluent-ish design
- Ratings & reviews display

**Could (v3)**
- Bulk install (`winget import`) and export
- Toast notifications (update available, install done)
- Package pinning / ignoring updates (`winget pin`)
- Offline / air-gapped install from downloaded MSIX

**Won't (in-app, v1–v3)**
- Purchases, checkout, Microsoft-account sign-in, and entitlement/license handling (paid apps get a **"Buy in Microsoft Store"** button that opens the official page in the browser)
- DRM'd content, Xbox/Game Pass integration

### 1.3 Use cases & personas

1. **Interactive desktop** — a nicer, faster Store: browse, install, update, remove free apps; no ads, no account walls.
2. **Home servers / headless boxes (primary)** — Windows Server (and debloated PCs) where the real Store app is absent or unwanted, but apps still need to be installed and kept updated via winget/MSIX. Needs: no Store dependency, works via winget manifests, and (ideally) **remote management** from another machine.
3. **Admin / reproducible setups** — install/update a fixed set of apps from an import file, with clear logs.

---

## 2. Research — prior art & building blocks

### 2.1 Existing open-source projects (reuse, don't reinvent)

| Project | Lang | Relevance |
|---|---|---|
| **winget-cli** (`microsoft/winget-cli`) | C++ | The package manager itself. MIT. Installs MSIX + Win32, has `msstore` source, REST sources, pin/export. **Primary execution backend.** |
| **winget-pkgs** (`microsoft/winget-pkgs`) | YAML manifests | Community catalog of ~tens of thousands of manifests. MIT. **Primary metadata source for Win32 apps.** |
| **winget.com** (`microsoft/winget.com`) | TS | Microsoft's own web storefront that hits the Store catalog API. **Best reference for the catalog REST shape.** |
| **UniGetUI** (formerly WingetUI) | C# | OSS GUI wrapping winget + Scoop + Chocolatey + npm/pip. **Feature/UX reference and proof the model works.** |
| **StoreLib** | C# | Community client for the Store catalog API. **Reference implementation for catalog access.** |
| **msix-packaging** (`microsoft/msix-packaging`) | C/C++ | MSIX SDK (makeappx, msixmgr). For packaging *ourselves*. |
| **Scoop** / **Chocolatey** | PS / C# | Extra installer sources, trivially added via CLI. |

### 2.2 Key APIs and integration points

1. **winget community catalog (REST-ish):** manifests live in the `microsoft/winget-pkgs`
   GitHub repo. Machine-readable catalogs are also exposed by `winget.run` and
   `winget-api.dev` REST endpoints. Use these for *fast, typed search* (much better than
   scraping `winget` stdout).
2. **Microsoft Store catalog (msstore source):** winget-cli talks to a Store REST service
   (`storeedgefd.dsx.mp.microsoft.com/v9.0/...`, "DisplayCatalog") to resolve Store apps to
   MSIX downloads — **no sign-in required for free apps**. Semi-public and not officially
   documented: treat as best-effort and hide behind an adapter trait.
3. **Package deployment:** `Windows.Management.Deployment.PackageManager` (WinRT) via the
   official Rust `windows` crate — add/remove/register MSIX, query installed packages.
   No need to P/Invoke PowerShell.
4. **Execution shortcut:** shell out to `winget` for install/upgrade (like UniGetUI does).
   It is battle-tested, handles MSIX + EXE/MSI + dependencies + agreements + hashes.

### 2.3 Rust stack

**UI — two realistic routes:**

| Route | Pros | Cons | Verdict |
|---|---|---|---|
| **Tauri 2** (WebView2 + Rust core) | Fastest path to a polished, Store-like UI; huge plugin ecosystem (notifications, updater, shell); Rust does the heavy lifting | Ships a web frontend; slightly larger footprint | **Recommended** |
| **iced** or **Slint** (native Rust UI) | Pure Rust, no web tech, smallest binary | Much more work for a rich, card-based store UI | Great later / if "pure Rust" is a hard requirement |

**Backend dependencies (crates):**
- `windows` (WinRT deployment, toasts, app model) — official MS bindings
- `reqwest` + `serde` + `serde_json` — catalog REST clients
- `tokio` — async runtime
- `sqlx` or `rusqlite` — local cache/settings/history
- `anyhow` / `thiserror`, `tracing` — errors & logging
- Tauri plugins: `shell`, `notification`, `updater`, `store`

**Packaging/distribution:** MSIX (via MSIX Packaging Tool / makeappx) or NSIS installer;
self-update via Tauri updater.

---

## 3. Proposed architecture

```
┌─────────────────────────────────────────────────────────┐
│  UI (Tauri frontend — Svelte/React/Solid, Fluent theme) │
└───────────────────────┬─────────────────────────────────┘
                        │ typed IPC (tauri::ipc / commands)
┌───────────────────────▼─────────────────────────────────┐
│  Core domain (Rust): Package, Source, InstallState,     │
│  CatalogService, InstallService, UpgradeService         │
└──────────┬──────────────────────────────┬───────────────┘
           │ trait PackageSource          │ trait PackageExecutor
  ┌────────┴─────────┐             ┌──────┴──────────────┐
  │ SourceAdapter    │             │ Executors           │
  │  • WingetRepo    │             │  • winget-cli (sub) │
  │    (REST/gh)     │             │  • PackageManager   │
  │  • MsStore       │             │    (windows-rs)     │
  │  • Scoop/Choco   │             │  • (Scoop/Choco)    │
  └──────────────────┘             └─────────────────────┘
            │ storage / cache
  ┌────────▼─────────┐
  │  SQLite (cache,  │
  │  settings, logs) │
  └──────────────────┘
```

**Key design rule:** every source and every executor sits behind a small Rust trait so the
semi-public msstore catalog (most likely to break) can be swapped or dropped without touching
the rest of the app.

**Recommended v1 approach:** catalog via winget community REST + msstore REST (both pure HTTP),
execution via `winget-cli` subprocess. This de-risks the project enormously — you reuse the
hardest, most-tested part instead of reimplementing installer logic.

---

## 4. Roadmap

### Phase 0 — Spike (1–2 weeks)
Prove the risky parts before committing:
- [ ] `winget` subprocess install/uninstall driven from Rust
- [ ] Search results from winget community REST (`winget-api.dev` / manifests)
- [ ] msstore catalog fetch returns an MSIX download for a free app
- [ ] `PackageManager` via `windows` crate lists/adds/removes an MSIX
- Decision gate: Tauri vs iced (prototype one screen in each)

### Phase 1 — MVP (4–6 weeks)
- Search + detail page + install/uninstall + progress for **winget community apps**
- Local SQLite cache of results
- `winget` as executor
- Ship a signed-ish installer; self-update stub

### Phase 2 — Store parity (4–6 weeks)
- Add **msstore source** (free apps only), screenshots, ratings
- **"Buy in Microsoft Store"** deep-link fallback for any paid item → opens official page in browser
- Library, update check, update-all
- Category browsing, dark/light theme, Fluent polish
- Pin/ignore apps

### Phase 3 — Extend (ongoing)
- Scoop + Chocolatey sources
- Bulk import/export, offline MSIX install
- Toast notifications, background update scan (scheduled task)
- **Headless / home-server mode** — optional small web UI (axum) or CLI so a server box can be managed from another machine

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| No official Store catalog API; endpoints change/throttle | Use winget community repo as the *primary* source; msstore as best-effort behind a trait; abstract + fallback |
| winget not present on some machines | Detect and offer bundled winget, or use `PackageManager` directly for MSIX |
| Paid/DRM apps cannot be supported | Explicit descope + messaging; focus on free-app excellence |
| Semi-public endpoints = ToS gray zone | Primary ship on winget community repo; make msstore an opt-in "extra source" |
| Native Rust UI effort blows up | Default to Tauri; keep iced/Slint as a later backend-swap |
| Trademark / naming | Pick an original name; no Microsoft marks or logos |
| Store app absent on Windows Server / no MSIX infra | Primary path on Server is winget EXE/MSI manifests; test MSIX early on Server |

---

## 6. Suggested repo layout

```
store/
├── src-tauri/            # Rust core: domain, sources, executors, commands
│   ├── src/
│   │   ├── domain/       # Package, Source, InstallState
│   │   ├── sources/      # winget_repo.rs, msstore.rs, scoop.rs
│   │   ├── executors/    # winget_cli.rs, package_manager.rs
│   │   ├── db/           # sqlite
│   │   └── commands.rs   # Tauri IPC surface
│   └── Cargo.toml
├── ui/                   # Tauri frontend (or replace with iced later)
└── docs/                 # ADRs, source notes, API notes
```

---

## 7. Open questions to resolve in Phase 0

1. Tauri (web UI) vs iced (pure Rust) — hard requirement or preference?
2. Windows version floor: Win10 1809+ (winget available) or Win11-only?
3. Is the msstore **source** acceptable for v1, or stay strictly on winget community repo?
4. Product name / branding (avoid Microsoft marks).
5. Distribution: MSIX (store-like, sandboxed) vs EXE installer vs portable?
6. Home-server management: built-in web UI, a separate CLI, or just run the desktop app over RDP?