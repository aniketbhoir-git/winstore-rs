# Catalog sources for issue #16 (community catalog without winget.run)

Research date: 2026-11-25. Measured on a Windows 11 machine with plain HTTPS requests (curl) and Python. No winget run was needed.

## Summary

| Option | Freshness | License / terms | Size | Offline | Effort in Rust |
|---|---|---|---|---|---|
| A. `winget` CLI (existing executor) | Same as winget's own index | MIT code; data is winget's | none | Yes, after winget updated its index | About 300 lines + text parser (ADR 0001) |
| B. Official index `source2.msix` + per-package files on the Microsoft CDN | Hours (CDN file changed 2026-11-24 17:55 GMT) | MIT client; no separate terms text found | 4.07 MB download, 9.27 MB `index.db` | Yes, fully | Medium: zip + SQLite + inflate + YAML |
| C. `microsoft/winget-pkgs` on GitHub (raw files / API) | Minutes (last push 2026-11-24 19:04 GMT) | MIT | Repository 991 MB (do not clone) | Only what you cache | Low for one package, bad for search |
| D. REST source contract (`winget-cli-restsource`) | Depends on the host you run | MIT reference code | n/a | No | High: you must host it |
| E. winstall / winget.run style web APIs | winget.run frozen 2023; winstall API closed | winstall site GPL-3.0; API undocumented | n/a | No | Not an option |

## B. Official index on the Microsoft CDN

### What it is

- The default `winget` source is a "pre-indexed" source. Its base URL is hard-coded in winget as `https://cdn.winget.microsoft.com/cache`.
  Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/SourceList.cpp (`s_Source_WingetCommunityDefault_Arg`).
- Two index files exist on 2026-11-25:
  - `https://cdn.winget.microsoft.com/cache/source.msix`: 22,425,983 bytes (older, schema 1.x).
  - `https://cdn.winget.microsoft.com/cache/source2.msix`: 4,071,168 bytes (schema 2.x).
  Source: HTTP HEAD responses on 2026-11-25. Both have `Last-Modified: Tue, 24 Nov 2026 17:55:56 GMT`, `Cache-Control: public, max-age=900`, an `ETag` and `Accept-Ranges: bytes`.
- An `.msix` file is a ZIP file. `source2.msix` holds `Public/index.db` (9,269,248 bytes), an `AppxManifest.xml`, a block map and a signature. Source: I listed the archive with Python `zipfile`.

### What the index contains (verified)

- `index.db` is SQLite. The `metadata` table says `majorVersion=2`, `minorVersion=1`.
- `packages` has 16,504 rows. Columns: `rowid, id, name, moniker, latest_version, arp_min_version, arp_max_version, hash`.
- Other tables: `tags2` (25,050), `tags2_map`, `commands2`, `commands2_map`, `pfns2`, `productcodes2`, `norm_names2`, `norm_publishers2`, `upgradecodes2`.
- Example row: `VideoLAN.VLC`, name "VLC media player", moniker `vlc`, latest version `3.0.24`. The current VLC version is correct here (the winget.run data said 3.0.18).
- The index has no publisher text, no description and no license. Those are in the per-package files below.

Schema source code (MIT): https://github.com/microsoft/winget-cli/tree/master/src/AppInstallerRepositoryCore/Microsoft/Schema/2_0
Design note on the object stack: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/Microsoft/README.md

### The per-package files (this corrects ADR 0001 option B)

ADR 0001 says the per-package files use "a binary format with no specification". I checked, and the chain is open and simple:

1. In `packages`, the `hash` column holds a SHA-256 for the package.
2. The version list is at `https://cdn.winget.microsoft.com/cache/packages/<PackageId>/<first 8 hex chars of hash>/versionData.mszyml`. For VLC: `.../packages/VideoLAN.VLC/9f8c3bb2/versionData.mszyml` (803 bytes, HTTP 200).
   Source for the path rule and file name: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCommonCore/PackageVersionDataManifest.cpp (`GetRelativeDirectoryPath`, `VersionManifestCompressedFileName`).
3. The file is a small Windows "Compression API" container (magic `0A 51 E5 C0`) around MSZIP data. MSZIP is `CK` plus raw deflate. I decoded it with Python `zlib.decompressobj(-15)` after the `CK` marker. The result is plain YAML: `sV: 1.0`, then `vD:` with entries `v` (version), `rP` (relative path of the manifest) and `s256H` (SHA-256 of the manifest).
   Source for MSZIP: same file (`CompressionAlgorithm = COMPRESS_ALGORITHM_MSZIP`).
4. The manifest is at `https://cdn.winget.microsoft.com/cache/<rP>`, for example `.../cache/manifests/v/VideoLAN/VLC/3.0.24/16cd`. It is plain UTF-8 YAML (merged manifest, 7,011 bytes). Its SHA-256 equals the `s256H` value in step 3 (`629c5bc6...`). I checked this.
5. The manifest has `PackageName, Publisher, Moniker, License, LicenseUrl, PackageUrl, PublisherUrl, Tags, Commands, ShortDescription, Description, ReleaseDate, Installers (URL, SHA-256, type, scope, arch)`.
   The manifest format is documented: https://learn.microsoft.com/en-us/windows/package-manager/package/manifest and the schema (includes an `Icons` field): https://github.com/microsoft/winget-cli/blob/master/schemas/JSON/manifests/latest/manifest.defaultLocale.latest.json

So one public chain gives search, details, the version list, the installer URL and hashes. Every step after the index can be verified by hash.

### Risk

- The CDN layout and the index schema are not a published API. I found no stability promise. The schema moved from 1.x to 2.x, and winget has an experimental delta index (`Delta Index.md` in https://github.com/microsoft/winget-cli/tree/master/doc/specs).
- Mitigation: read `metadata.majorVersion`. If it is not 2, stop and fall back to option A. Keep a fixture index in the repo for tests.
- I found no terms of use for direct CDN download. The URL is the default source of an MIT-licensed client, and the data comes from an MIT repository (see C). Ask the maintainer to accept this risk in the ADR.
- `source2.msix` carries a package signature (`AppxSignature.p7x`). I did not find where winget checks it, and I did not test it. Do not claim signature checking. Use HTTPS and the hash chain.

### Freshness and offline use

- The CDN file changed on 2026-11-24 17:55 GMT and the `winget-pkgs` repository was pushed at 19:04 GMT the same day. So the index lags the repository by hours at most.
- `max-age=900` means 15 minutes. Use `If-None-Match` with the `ETag`. A refresh costs a 4 MB download at most.
- After one download the search works offline. Detail pages need a per-package fetch (or a local cache) and cache well because the files are hash-addressed.

### Rust effort

- `zip` (read MSIX), `rusqlite` with `bundled` (read `index.db`), `flate2` or `miniz_oxide` raw inflate (MSZIP), `sha2` (verify), a YAML crate (`serde_yaml`-compatible) for manifests, `reqwest` already present.
- Skip the container header (28 bytes up to and including the `CK` marker in my sample) and inflate. I tested only a single-block file (803 bytes). I did not test larger files, which can hold several blocks. The container is the Windows Compression API format: https://learn.microsoft.com/en-us/windows/win32/cmpapi/using-the-compression-api
- Search code: about 150 lines for the ranking ladder in `search-ranking.md`.

## C. `microsoft/winget-pkgs` on GitHub

- The repository is MIT licensed, not archived, 991,359 KB, and has about 7,180 open issues.
  Source: GitHub API `GET /repos/microsoft/winget-pkgs` (fields `license.spdx_id`, `size`, `pushed_at`, `archived`) on 2026-11-25.
- Manifests are at `manifests/<first letter>/<Publisher>/<Package>/<Version>/<Id>.yaml` (plus `.installer.yaml` and `.locale.<lang>.yaml`). Example fetched: `https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests/v/VideoLAN/VLC/3.0.24/VideoLAN.VLC.installer.yaml` (200, 6,070 bytes).
- The GitHub REST API allows 60 requests per hour without a token. Source: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- Cloning is not practical for an app. There is no text search endpoint for this use.
- Use: a fallback for one package's details (release notes, icons if the manifest has them). Not for search.

## D. REST source contract (`winget-cli-restsource`)

- The reference implementation is MIT, not archived, last push 2026-10-01. Source: https://github.com/microsoft/winget-cli-restsource
- The OpenAPI file (`documentation/WinGet-1.9.0.yaml`, up to 1.10.0) defines `/packages`, `/packageManifests/{id}`, `POST /manifestSearch` and `/information`. Source: https://github.com/microsoft/winget-cli-restsource/tree/main/documentation
- It is a server you host (the repo targets Azure). There is no public community instance run by Microsoft. winget's community source does not use REST. The Microsoft Store source does (`msstore`, see `search-ranking.md`).
- Useful as a client contract if the project later hosts its own mirror or adds the Store source. Not a catalog by itself.

## E. Web APIs (winstall, winget.run, others)

- winget.run: archived 2023, data frozen at 2023-03-16 (see `search-ranking.md`).
- winstall: the site README says its API checks `winget-pkgs` every 15 minutes on weekdays, and that the API is not open source yet. The repository (`SplashtopInc/winstall`) is GPL-3.0 and covers the web front end. No documented public API.
  Source: https://github.com/SplashtopInc/winstall (README, "How is the data obtained?").
- ADR 0001 already records `winget-api.dev` (connection failure), `api.winget.pro` (404) and Repology (no VLC rows). I did not repeat those tests.

## Recommendation

Use option B: read the official `source2.msix` index locally for search, and read the hash-verified `versionData.mszyml` and manifest YAML for details.
It is fresh to the hour, works offline, needs no winget on the machine and no live third-party service, and it fixes #20 with a ranking ladder that copies winget; keep option A (winget CLI) as the fallback when the index major version is not 2, and amend ADR 0001 because its claim that per-package files are an unspecified binary format is wrong.

## Features worth building

- **Local catalog from the official winget index.**
  Download `source2.msix` with `ETag` checks, extract `index.db`, and search it. This closes #16 and #20 without a third-party service.
- **Hash-verified package detail loader.**
  Load `versionData.mszyml` and the manifest YAML, and check each SHA-256. Detail pages then show publisher, license, tags, installers and release date with trusted data.
- **Index schema guard and fallback.**
  Read `metadata.majorVersion`. On an unknown version, show a clear message and fall back to `winget search`. This protects users from silent breakage.
- **Fixture index for automated tests.**
  Keep a small `index.db` and two manifests in the repo. Tests for "vlc" and a no-match query run offline (F1 acceptance).
- **Background index refresh with a "last updated" label.**
  Refresh at most every 15 minutes with `If-None-Match`. Show the age of the catalog so users know it is fresh.
- **Amend ADR 0001.**
  Record the measured chain (index, hash, versionData, manifest) and replace "binary format with no specification". The maintainer decides the final option.
