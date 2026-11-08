# Store (working title)

An open-source, Rust-based app storefront and package-manager front end for Windows.
Find, install, update, pin, and remove applications across multiple sources — the winget
community repository, the Store catalog (via winget's `msstore` source), Scoop, and Chocolatey —
with a clean, fast UI and an optional headless mode for managing Windows Server machines.

> Not affiliated with or endorsed by Microsoft. Product names and logos belong to their owners.

## Status

Early and in progress. This is deliberately a **slow-moving, community-built project** — no
deadlines, contribution is asynchronous, and every piece of work is a self-contained issue that
anyone (on any model tier) can pick up. See `docs/ISSUE-BACKLOG.md`.

## How it works

- **Browse & search** catalogs from multiple sources with one merged, typed result set.
- **Install / remove / update** delegate to `winget` (and Scoop/Chocolatey) as subprocesses;
  MSIX packages can also be deployed directly via the Windows Deployment API.
- **Purchases stay outside the app.** A paid item shows a "Buy in Microsoft Store" button that
  opens the official product page in your default browser. No payment, account, or
  licensing/entitlement logic lives in this codebase.
- **Headless mode (planned):** a localhost web UI to manage headless Windows Server boxes.

## Stack

Rust · Tauri 2 · Svelte/TypeScript · SQLite · tokio · windows-rs

## Contributing

Contributors can donate their **LLM tokens** instead of (or in addition to) hours: each
implementation issue is a paste-ready prompt. Start here:

- [`CONTRIBUTING.md`](CONTRIBUTING.md)
- [`docs/CONTRIBUTING-WITH-LLM.md`](docs/CONTRIBUTING-WITH-LLM.md)
- [`docs/SPEC.md`](docs/SPEC.md) — the LLM hand-off spec (safe to paste into any model)
- [`docs/ISSUE-BACKLOG.md`](docs/ISSUE-BACKLOG.md) — pick an issue

## License

MIT — see [`LICENSE`](LICENSE).