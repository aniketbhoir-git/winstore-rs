# ADR 0001 — Community catalog backend

- **Status:** proposed. The maintainer has not picked an option.
- **Date:** 2026-11-11
- **Closes when accepted:** F1 (GitHub #20), follow-up #16, review action A14.
- **Measured on:** Windows 11 build 22631, winget 1.29.380, one machine, English locale.

## Problem

`sources/winget_repo.rs` reads `https://api.winget.run/v2/packages`. That service stopped at
2023-03-16. A query for "vlc" returns Flock and f.lux, and VLC shows version 3.0.18 when the
current version is 3.0.24. The adapter passes the query correctly. No change in our code can
repair the data. The app needs a different backend for search and for the detail page.

## Constraints

- Public, documented interfaces only. HTTPS, read-only. No scraping.
- No telemetry. No token or secret in the shipped app.
- An automated test must not depend on a live service.
- F1 acceptance: "vlc" returns `VideoLAN.VLC` in the first 3 results; a query with no match
  returns an empty list.

## Options

### A. winget CLI: `winget search` and `winget show` through the existing executor

- **Search:** 0.32 to 0.39 s for "vlc" (3 runs). `VideoLAN.VLC` is the first row. winget ranks
  the rows itself.
- **Detail:** `winget show` gives version, publisher, description, homepage, license, tags,
  installer type, installer URL, SHA256, release date. `--versions` gives the version list.
- **Cost of the first detail read:** 21 to 23 s for each package that winget did not read
  before (3 of 3 packages). Later reads take 0.3 s. The cause is not known. It can be this
  machine.
- **No JSON output.** The app must parse a text table and a key/value block. The header
  strings come from the Windows language. Only English was tested.
- **Table truncation:** winget 1.29 prints full columns when the output goes to a pipe. Older
  versions are not tested. The project minimum is winget 1.6.0 today.
- **Absent fields:** icon, screenshots, rating, installer size.
- **Tests:** the fake winget fixture exists. Tests run with recorded output and no network.
- **Size of the change:** about 300 lines plus fixtures. The table parser and the process
  control exist.

### B. Read the official winget index directly (`source2.msix` from the Microsoft CDN)

- The file is 3.8 MB and a few hours old. It holds a SQLite database with ids, names,
  monikers and tags.
- It holds no publisher name, description, license or version list. Those are in
  per-package files in a binary format with no specification.
- The only schema text is an internal design note in the winget-cli repository. It makes no
  promise of stability.
- **Fails the rule "public, documented interfaces only".**

### C. Another REST mirror

| Service | Result on 2026-11-11 |
|---|---|
| `api.winget.run` | Stopped in March 2023. |
| `winget-api.dev` (named in SPEC §0) | Connection failure. |
| `api.winget.pro` | 404. Not a catalog service. |
| `winget.ragerworks.com`, `winstall.app` | Web pages only. No documented API. |
| Repology | No winget rows for VLC. |
| GitHub API on `microsoft/winget-pkgs` | Current and documented. No text search without a token. 60 requests per hour without a token. |

No mirror can do search. The raw manifest files on GitHub can add detail fields (icon,
release notes) for one package at a time.

### D. winget COM API (`Microsoft.Management.Deployment`)

- Documented. Gives typed search results and metadata. No text parsing.
- The specification names packaged apps and higher-privilege processes as callers. This app
  is not packaged. Access from this app is not verified.
- The `windows` crate has no bindings for it. The project must generate them. This is a
  dependency outside SPEC §2.

### E. Own static catalog, built from `winget-pkgs` by a scheduled CI job

- A job reads the MIT-licensed manifests and publishes one JSON file on a static host. The app
  downloads it and searches locally.
- Works with no winget on the machine. Can carry icons where a manifest has them.
- The project then owns a data pipeline, a host and their failures. One maintainer keeps it
  alive. That is the same weakness that stopped `winget.run`.
- Not measured. No size or build time is known.

## Comparison

| | A CLI | B index | C mirrors | D COM | E own catalog |
|---|---|---|---|---|---|
| Follows the interface rule | Yes | No | No search | Yes | Yes |
| Current data | Yes | Yes | No | Yes | Yes, if the job runs |
| Search quality | winget's own | We write it | None | winget's own | We write it |
| Needs winget on the machine | Yes | No | No | Yes | No |
| Icons, screenshots, ratings | No | No | Icons only | No | Icons only |
| New code | About 300 lines | 1500 or more | n/a | 800 or more, plus bindings | Not measured |
| Risk that stays | Text format, locale | Format change | n/a | Access not verified | We run a service |

## Recommendation

Option A, with three conditions written into the fix issue.

What is wrong with option A comes first:

1. **Text parsing with no contract.** A winget update or a non-English Windows can break
   search with no warning. Mitigation: parse by header offsets, never by header words; add a
   fixture per winget version that the project supports; show `source_unavailable` when the
   parse finds no header line.
2. **The 21 to 23 s first detail read.** If winget causes it, each first open of a detail page
   is slow. Mitigation: build search cards from the search row only; load the detail with a
   progress state and a 60 s limit; keep the detail in the `packages` table with the existing
   time limit.
3. **No winget, no catalog.** The app needs winget to install, so the loss is small for the
   MVP. It is a real loss for a later "browse first" mode.
4. **No icons, screenshots or ratings.** A storefront shows these. Option A does not solve
   it. The detail page for a community package must work with no images (review action A16).

Why option A is still the first choice: it is the only option that follows the interface
rule, gives current data, ranks results correctly today, and reuses code that exists. Options
B and C fail a hard constraint. Options D and E cost much more and each has a question that
nobody answered yet.

Conditions:

1. Raise the minimum winget version for the catalog to 1.29, or test the parser on an older
   version first. This is a maintainer decision.
2. Before the fix merges, measure the first `winget show` on a second machine or a CI runner.
   If each machine shows 20 s or more, open option D or E again.
3. Test the parser with recorded output from one non-English Windows. If no such recording is
   available, state the limit in the README.

Option E stays open as the later answer for images and for a mode with no winget. It needs its
own decision record.

## Consequences if option A is accepted

- `sources/winget_repo.rs` and its `httpmock` tests go away or stay behind a disabled source.
  `reqwest` stays for WP7.
- SPEC §2 rationale ("the catalog layer is kept separate (pure HTTP)") is no longer true and
  needs an edit. SPEC §0 must drop `winget-api.dev` and `winget.run`.
- Search needs no cache: the local winget index answers in 0.3 s (review action A15).
- The package id in a search row is the winget id, so the id check before install (A14) is
  exact by construction.
- `PackageSummary` rating and icon fields stay empty for this source.

## Not verified

Header text on non-English Windows; truncation on winget older than 1.29; the cause of the slow
first `winget show`; whether the `winget` source asks for an agreement on a new machine; COM
access from an app with no package identity.
