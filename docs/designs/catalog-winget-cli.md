# Design — community catalog through the winget CLI (ADR 0001 option A)

- **Status:** draft, not reviewed. Run `/egm:egm-review docs/designs/catalog-winget-cli.md`
  before code, if the owner wants a review.
- **Date:** 2026-11-25
- **Issues:** F1 (GitHub #20), #16, review actions A14 and A15.
- **Decision record:** `docs/adr/0001-community-catalog-backend.md`, option A, accepted
  2026-11-25 by the orchestrator in GTAVI mode (owner away). The owner may reverse it.
- **Owner gate:** Step 4 changes SPEC §0 hard rule 2 (HTTPS only); the owner confirms before
  step 4 merges.

## Objective

Search and the detail page read the `winget` winget source through the winget CLI. The app
stops reading `api.winget.run`. Search results are current, and winget ranks them.

F1 acceptance:

1. A search for "vlc" returns `VideoLAN.VLC` in the first 3 results.
2. A search for a string with no match returns an empty list, not an error.
3. An automated test covers both. It uses recorded winget output. It needs no network and no
   real winget.

## Non-goals

- Icons, screenshots, ratings and installer size. Option A does not give them (ADR 0001).
- The `msstore` winget source. It has a source agreement and needs its own design.
- The winget COM API (option D) and an own catalog (option E).
- A version list on the detail page. The detail shows the latest version only (see
  "Decisions").
- A change to the parser of `winget list` and `winget upgrade`. That is WP6 work.

## Overview

```
UI ──search_sources──▶ Catalog (catalog/mod.rs) ──search()──▶ WingetCliSource
UI ──get_app_detail──▶ Catalog ──get_detail()──▶ WingetCliSource
                                                     │
                                                     ▼
                         WingetCli (executor/winget_cli.rs): read-only calls
                         `winget search ...`, `winget show ...`
                                                     │
                                                     ▼
                         winget_parse.rs: parse_columns(), parse_show()
```

The coordinator (`Catalog`) and its cache keep their shape. Only the adapter behind
`CatalogSource` changes, plus two small changes in the coordinator: the merge rule in
`catalog/search.rs` (section 3) and one new `Catalog` method for the cached detail
(section 5).

## Design

### 1. The new catalog source

- Add `sources/winget_cli.rs` with `WingetCliSource`. It implements `CatalogSource` for
  `SourceKind::WingetCommunity`.
- The source keeps the same `SourceKind`. Package ids thus stay
  `winget_community/<winget id>`. Ledger rows and the install path do not change.
- The source holds a lazy `WingetCli` (a `tokio::sync::OnceCell`). It connects at the first
  search, not at app start. With no winget on the machine, the search returns
  `winget_not_found`. The app still starts.
- Capabilities: `search` and `detail` are true. `featured` and `categories` are false. The
  default trait methods return `unsupported` for them.
- `ipc/state.rs`, `enabled_sources()`: the `WingetCommunity` arm builds `WingetCliSource`.
  It no longer builds `WingetRepoSource`.

**What happens to `sources/winget_repo.rs`.** The module stays in the tree. Nothing builds
it. Its unit tests and `tests/catalog_http.rs` stay and keep running. Its module comment says
"not wired; replaced by `winget_cli.rs` (ADR 0001)". To reverse the decision, change one line
in `enabled_sources()`. A later pull request deletes the module after the owner confirms
ADR 0001.

### 2. The read-only winget calls

Add two methods to `WingetCli`. They use `run_capture_timeout()`, which exists. They do not
take the `OpGuard`, because they change nothing.

| Call | Arguments (in this order) | Time limit |
|---|---|---|
| Search | `search --query <text> --source winget --count <n> --disable-interactivity` | 15 s |
| Detail | `show --id <id> --exact --source winget --disable-interactivity` | 60 s |

Rules for each call:

1. `--source winget` is always present. winget then reads only the community source. It does
   not touch `msstore`, which has a source agreement ("Terms of Transaction"). It also does
   not print the "Source" column.
2. `--query <text>` carries the search text. A text that starts with `-` is thus never read
   as a flag. The text goes as one argument; no shell runs.
3. `--count <n>` is `SearchQuery::fetch_limit()` (1 to 100). A one-letter query cannot return
   12,000 rows.
4. `--disable-interactivity` is always present. winget then never waits for input. A prompt
   becomes "not accepted" and a non-zero exit code.
5. No agreement flag, ever. `guard_args()` and `FORBIDDEN_FLAGS` already refuse
   `--accept-source-agreements` and `--accept-package-agreements`. A unit test checks the two
   argument lists against the guard.
6. The search text is trimmed. An empty text returns an empty list and starts no process (the
   coordinator does this today).

Exit codes for the two calls:

| Exit code | Search result | Detail result |
|---|---|---|
| 0 and a table or a key block was parsed | The rows | The detail |
| 0 and nothing was parsed | `source_unavailable` ("winget output not recognized") | Same |
| `0x8A150014` (no applications found) | Empty list | `package_not_found` |
| `0x8A150046` (source agreements) | `agreements_required`, source kind, `winget_source` = `winget` | Same |
| `0x8A150041` (package agreements) | Not expected. Same mapping as `map_exit()` | Same |
| Time limit reached | `source_unavailable` ("winget did not answer in 15 s") | `source_unavailable` ("winget did not answer in 60 s") |

`run_capture_timeout()` in `winget_cli.rs` builds the time-limit error today. It says "winget
did not answer in time", with no number. Step 2 changes it to put the limit of the call in
the message, as the table shows.
| Any other | `map_exit()`, as today | Same |

### 3. Search: parse by column position

Add `parse_columns(text) -> Option<Columns>` to `executor/winget_parse.rs`. It finds the
columns by position. It never reads a header word.

1. Split the text into lines. Drop empty lines and spinner lines (`is_spinner()` exists).
2. The **separator line** is the first line that has only `-` characters and at least 10 of
   them. No separator line: return `None`.
3. The **header line** is the line directly above the separator. A column starts at each
   character offset in the header where a non-space character follows a space, and at offset
   0. The header words are not read.
4. Each line below the separator is a data row. A cell is the text from its column start to
   the next column start, trimmed.
5. A row is skipped, with a warning in the trace log, when:
   - a column start does not follow a space in that row (wide characters move the offsets;
     the existing `parse_table()` has the same rule);
   - the cell at position 1 is empty, has a space in it, has no `.` in it (every winget id
     has a `.`), or ends with `…` (a cut id);
   - the cell at position 2 is empty.
6. The **cut-list footer** is detected on its own. winget prints a line that starts with `<`
   when it cut the result list. The parser ends the data rows at the first line that starts
   with `<`. It does not rely on the row rules above for this line. A recording from winget
   1.29.380 (stdout a pipe, GitHub runner, 2026-11-25) shows no cut cell and no footer for
   ten rows; the longest line is 77 characters. The footer text is still from the winget
   source, not from a recording.
   A row can have an empty Match cell; the parser drops empty extra cells.
7. The parser returns the rows and the warnings.

The search maps the cells by position:

| Position | Meaning | Use |
|---|---|---|
| 0 | Name | `PackageSummary.name` |
| 1 | Id | `PackageId::new(WingetCommunity, id)` |
| 2 | Version | `PackageSummary.latest_version` |
| 3 and later | Match, Source | Ignored |

The other summary fields: `publisher` and `description` are empty strings. `icon_url`,
`rating` and `rating_count` are `None`. `is_paid` is false. `packaging` is `Unknown`. The
detail fills publisher, description and packaging later.

**A search row must not overwrite a stored publisher or description.** `UPSERT_SUMMARY` in
`db/repo/packages.rs` sets `publisher` and `description` to `excluded.*` on conflict. A search
row would then erase the values that an earlier `show` stored. Step 4 changes the conflict
clause for these two columns: `publisher = COALESCE(NULLIF(excluded.publisher, ''),
publisher)`, and the same for `description`. A non-empty stored value stays. A non-empty new
value (from a detail write) replaces it. A test writes a detail, then a search row with empty
fields, and checks that publisher and description stay.

**Duplicates in the merge.** Today `merge()` in `catalog/search.rs` drops a later row when
`dedupe_key()` (name and publisher) repeats. It does this across the flattened list, also for
rows of one source. A winget search row has an empty publisher. Two packages with the same
display name then collapse into one, and one of them cannot be found. Step 3 changes
`catalog/search.rs`:

1. Inside one source, `merge()` dedupes by `(source, id)` only.
2. Across sources, `merge()` applies the name-and-publisher rule only when both rows have a
   non-empty publisher. A row with an empty publisher is never dropped by this rule.

`dedupe_key()` returns `None` for an empty publisher. A unit test in `search.rs` uses two
winget rows with the same name and different ids and an empty publisher; both stay. A second
test keeps the old cross-source case (same name and non-empty publisher in two sources gives
one row). The fixture `search-same-name.txt` (see "Tests") feeds the integration test.

The result keeps winget's order. `SortOrder::Relevance` in the coordinator keeps it too.
winget puts an exact id, name or moniker match before a substring match. For "vlc", the
moniker `vlc` puts `VideoLAN.VLC` first (`docs/research/2026-11-25-search-ranking.md`).

### 4. Detail: parse the `winget show` block

Add `parse_show(text, id) -> Option<ShowBlock>` to `executor/winget_parse.rs`.

1. The **title line** is the first line that ends with `[<id>]`, where `<id>` is the
   requested id (case-insensitive). No title line: return `None`. The name is the text
   before ` [<id>]`, without its first word ("Found" in English).
2. Each line after the title with no indent and the form `Key: value` starts a field.
3. A line with an indent continues the field above it. Under `Tags:` each such line is one
   tag. Under `Installer:` each indented `Key: value` line is an installer field.
4. A key table maps English keys to fields. A key that is not in the table is ignored.

| Key | Field |
|---|---|
| `Version` | `summary.latest_version`, and the one entry of `versions` |
| `Publisher` | `summary.publisher` |
| `Description` | `full_description`; its first line, cut at 160 characters, is `summary.description` |
| `Homepage` | `website` |
| `License` | `license` |
| `Release Notes` | `release_notes` |
| `Tags` | Not stored today (`PackageDetail` has no tags field) |
| `Installer Type` | `installer.install_type` (`msix`, `msi`, `exe`, `portable`; other values give `Unknown`) |
| `Installer Url` | `versions[latest].installer_url` |
| `Installer SHA256` | `installer.sha256` and `versions[latest].sha256` |
| `Release Date` | `versions[latest].release_date` |

The key table is the one place in this design that reads words. On a non-English Windows the
keys differ. Then the title line and the id still parse, and the detail has only name, id and
the version from the cached search row. This is a stated limit (section 7).

The detail text is untrusted. The UI shows it as text, never as HTML (SPEC §6.6, "Trust
boundary"). The UI does this today; this design adds no new HTML path.

### 5. Latency of the first `winget show`

ADR 0001 measured 21 to 23 s for the first `show` of a package on one machine. Later reads
took 0.3 s.

1. Search cards use the search row only. A search never runs `show`.
2. `get_app_detail` runs `show` with a 60 s limit. At the limit it returns
   `source_unavailable`, retryable.
3. The detail page shows a loading state while the call runs: a spinner and the text
   "Loading details from winget. The first time can take up to a minute." The UI has no
   progress number to show, so it shows none.
4. The detail goes into the `packages` table with `detail_fetched_at`, as today
   (`packages::upsert_detail()`). A second open within the cache time limit runs no process.
5. `start_install` uses the cached detail when one exists, also when it is expired. It needs
   the detail only for the paid check and the target version. A community package is never
   paid, and winget installs by exact id. With no cached detail it calls `get_detail()` as
   today. This builds the planned part of SPEC §6.2, step 1. Code: a new public method
   `Catalog::cached_detail_any_age(&self, id: &PackageId) -> Option<PackageDetail>` in
   `catalog/mod.rs` (where `Catalog` and the private `cached_detail()` live). It reads the
   `packages` row and ignores `detail_fetched_at`. `start_install` in `services/install.rs`
   calls it first and falls back to `get_detail()`.
6. Condition 2 of ADR 0001: before the fix merges, measure the first `show` on a second
   machine (see step 6 of the plan). If each machine shows 20 s or more, the owner reopens
   option D or E.

### 6. Read-only calls and agreements (SPEC §6.6)

SPEC §6.6, "Read-only calls", requires this work to define the consent step for search and
detail before code. The step is: **no consent step; the calls fail closed.**

1. The calls name `--source winget`. The `winget` community source declares no source
   agreement. Only `msstore` does in a default winget setup (`docs/research/
   2026-11-25-install-progress-and-agreements.md`, section 3).
2. `search` and `show` never install, so winget does not ask for package agreements. `show`
   can print an "Agreements" block as data; the parser ignores it.
3. `--disable-interactivity` turns any prompt into "not accepted" and an exit code. winget
   never waits for input that nobody can give.
4. The app never passes an agreement flag to a read-only call. `guard_args()` enforces this.
5. When winget still asks for an agreement (exit `0x8A150046` or `0x8A150041`), the command
   returns `agreements_required`. The UI shows it as an error, as SPEC §6.6 says today. The
   message says: "winget needs you to accept the terms of the winget source. Run `winget
   search` once in a terminal, read the terms, and accept them there." The app opens no
   consent dialog for a read-only call.
6. The real-winget check of step 6 proves point 1 on a fresh machine (a CI runner). It runs
   `winget search vlc --source winget --disable-interactivity` with no agreement flag and
   expects exit code 0. If it fails, stop and return to the owner: a consent step for
   read-only calls is then necessary, and it follows the rules of §6.6.

### 7. Minimum version and the non-English limit

- **Minimum winget 1.29** (SPEC §2). `MIN_VERSION` in `winget_parse.rs` changes from 1.6 to
  1.29. This also moves the floor for installs and upgrades, because `probe()` serves all
  calls. Step 2 updates `version_parsing_and_floor` in `winget_parse.rs` and
  `version_probe_enforces_floor` in `tests/executor_winget.rs` (lines 526 to 535, which
  assert "1.6"). `probe()` then returns `unsupported` ("winget 1.29 or newer is necessary") for an older
  winget. The catalog source calls `probe()` once, when it connects. Reason: winget 1.29
  prints full columns to a pipe. Older versions are not tested and can cut cells.
- **Non-English Windows.** The column parser does not read header words. The `show` key table
  does. On a non-English Windows, whether the search works is **unverified**. A translated
  header word that holds a space adds a fake column start, so the position map shifts. The
  detail shows only name, id and version. Nobody has a non-English recording today. The README
  states the limit: "Search and app details are tested on English Windows only." A recording
  from another language becomes a new fixture.
- **Wide characters.** A name with CJK characters can move the column offsets. The row is
  skipped with a warning. This is the rule of `parse_table()` today.

### 8. Cache

- **Search.** The coordinator keeps its query cache and its time limit. The cache now holds
  current rows, so the cause of F1 is gone. ADR 0001 says search needs no cache (A15); a
  removal is a separate, later change.
- **Old rows.** The `packages` table can hold rows from `winget.run` with the same source
  kind. Migration `0004_clear_community_cache.sql` deletes them:
  `DELETE FROM packages WHERE source = 'winget_community';`. The table is a cache, so no user
  data is lost. The migration changes no schema.

## Tests

All tests run unattended, with no network and no real winget (SPEC §8, F2).

| Test | Where | What it checks |
|---|---|---|
| Column parser | `winget_parse.rs` unit tests | Recorded `winget search` output: with and without the Match column, a name with spaces, the cut-list footer (a line that starts with `<`), an id with no `.` skipped, CJK row skipped, no separator gives `None`. |
| Merge | `catalog/search.rs` unit tests | Two winget rows, same name, empty publisher, different ids: both stay. Same name and non-empty publisher in two sources: one stays. |
| Upsert | `db` tests | A search row with empty publisher and description does not overwrite stored values (`UPSERT_SUMMARY`). |
| Show parser | `winget_parse.rs` unit tests | Recorded `winget show VideoLAN.VLC` output: fields, tags, installer block, a multi-line description, an unknown key, a missing title gives `None`. |
| Argument lists | `winget_cli.rs` unit tests | Search and show args contain `--source winget` and `--disable-interactivity`, pass `guard_args()`, and hold no agreement flag. A query `-x` stays after `--query`. |
| F1: "vlc" | `tests/catalog_winget_cli.rs` | Fake winget prints the recorded "vlc" table. `Catalog::search("vlc")` has `VideoLAN.VLC` in the first 3. |
| F1: no match | same file | Fake winget prints "No package found matching input criteria." and exits `0x8A150014`. The result is an empty list. |
| Agreement | same file | Fake winget exits `0x8A150046`. Search returns `agreements_required`; the argv file shows no agreement flag. |
| Time limit | same file | Fake winget sleeps past a short test limit. Detail returns `source_unavailable`. The limit is a field, so the test does not wait 60 s. |
| Detail cache | same file | Two `get_detail()` calls start one `show` process. The test counts the lines of the argv file and excludes the `--version` call of `probe()`. |
| Cached detail on install | same file | `Catalog::cached_detail_any_age()` returns an expired detail and starts no process. |
| Old version | same file | `FAKE_WINGET_VERSION=v1.28.240` gives `unsupported`. |
| Migration | `db` tests | Migrations apply from an empty DB; 0004 removes community rows and keeps other rows. |

Fixtures go in `src-tauri/tests/fixtures/winget/1.29/` as plain text files:
`search-vlc.txt`, `search-none.txt`, `search-same-name.txt` (two packages with the same
name and different ids, and a cut-list footer), `show-vlc.txt`. One folder per winget version that the
project supports. The fake winget gets a new directive, `file PATH`, which prints a file as it
is. The fixtures are recorded once from winget 1.29.380 on English Windows. Each file starts
with no header of our own, so it stays an exact copy of winget's output.

## SPEC edits needed

These edits go in the same pull request as the code step that makes them true.

| Section | Edit |
|---|---|
| §0, terms | Replace the `api.winget.run` line: the community catalog is the `winget` winget source, read through the winget CLI (ADR 0001, accepted). |
| §0, hard rule 2 | Add: "or read-only calls to the winget CLI". The rule says HTTPS only today. |
| §2, minimum winget | Remove "The code and the skill say 1.6.0 today"; the check is 1.29. |
| §2.2, catalog row | "Proposed: (b)" becomes "Accepted 2026-11-25: (b)". |
| §2.3, F1 row | The mitigation is done. The 21 to 23 s row: 60 s limit, loading state, cache, cached detail on install. |
| §3, layout | Add `sources/winget_cli.rs`. Mark `winget_repo.rs` "not wired". |
| §4.3, `packages` | Community rows come from `winget show`; migration 0004 cleared the old rows. |
| §5.1 | An adapter may run read-only winget calls through `WingetCli`. Give the two time limits. |
| §5.2 | Add `search` and `show` to the read-only calls of `WingetCli`. They are not `PackageExecutor` methods. |
| §6.1 | Remove "The cache rule is pending ADR 0001". Describe the search row and the empty-list rule. |
| §6.2, step 1 | The planned part (cached detail on install) becomes built. |
| §6.6, "Read-only calls" | Add the decision of section 6: no consent step, `--source winget`, fail closed. |
| §6.7, gap table | The "recorded version" row: built by step 7, or moved to WP6 if step 7 waits. |
| §7, WP2 | Replace the "After the merge" note: the F1 fix replaced `winget_repo.rs`. |
| §8 | Add the fixture folder per winget version. The `httpmock` bullet applies to `winget_repo.rs` until its removal. |

Outside SPEC: `docs/ISSUE-BACKLOG.md` F1 (decision taken), `CLAUDE.md` status and layout, the
`winget-executor` skill (minimum 1.29, the two new calls), `README.md` (the English limit).

## Decisions

Each choice of taste below takes the option that is easiest to reverse.

1. **Keep `winget_repo.rs`, not wired.** Reverse: one line in `enabled_sources()`. Deletion
   waits for the owner.
2. **Same `SourceKind` (`WingetCommunity`).** Ids and ledger rows stay valid. A new kind would
   need a ledger migration.
3. **Clear the old cache rows with a migration.** The alternative, a version tag on each row,
   needs a schema change.
4. **Search rows have an empty publisher.** No guess from the id prefix. The detail fills it.
5. **The detail shows the latest version only.** No `--versions` call. A later step can add
   it with one more read-only call.
6. **Keep the search query cache.** Its removal (A15) is a separate change.
7. **New parser `parse_columns()` beside `parse_table()`.** The `list` and `upgrade` parser
   does not change in this work. WP6 can move it to the new parser.
8. **One minimum version (1.29) for the catalog and the executor.** SPEC §2 already sets it.
9. **No consent step for read-only calls; fail closed.** If the CI check finds an agreement
   prompt, the owner decides the next step.
10. **No cancel of an old search process.** It ends in about 0.3 s. The UI drops a late
    answer by `search_id`, as today.

## Risks

| Risk | Mitigation |
|---|---|
| A winget update changes the table or the show block. | Fixtures per winget version; `source_unavailable` when nothing parses; the real-winget check in CI. |
| The first `show` is slow on each machine. | Loading state, 60 s limit, cache. Step 6 measured a runner on 2026-11-25: first `show` 457 ms, second 258 ms. A second machine is still open. 20 s or more on each machine reopens option D or E. |
| Non-English Windows. | Column parser has no header words. The show key table does; stated in the README. |
| The `winget` source adds an agreement later. | Fail closed with `agreements_required`. No flag is passed. |
| Search spawns a process per query. | The UI debounces. Each search takes about 0.3 s. |
| No icons, screenshots or ratings. | The detail page works with no images (A16). Option E stays open. |

## Implementation plan

Each step is one pull request. Each step runs `cargo build`, `cargo clippy` and `cargo fmt`
in `src-tauri/`. The table gives the test command of the step.

| Step | Pull request | Test command (in `src-tauri/`) |
|---|---|---|
| 1 | **Parsers.** `parse_columns()` and `parse_show()` in `winget_parse.rs`. Record the four fixtures from winget 1.29.380. Unit tests only. | `cargo test --lib executor::winget_parse` |
| 2 | **Read-only calls.** `WingetCli::search_catalog()` and `show_package()`, the argument lists, the time limits as fields, the exit-code table, the time-limit message in `run_capture_timeout()`, `MIN_VERSION` = 1.29 with the updated floor tests (`version_parsing_and_floor`, `version_probe_enforces_floor`). Add the `file` directive to the fake winget. | `cargo test` |
| 3 | **Catalog source.** `sources/winget_cli.rs` (`WingetCliSource`, lazy connect). `tests/catalog_winget_cli.rs` with the F1 tests, the agreement test, the time-limit test and the cache test. The merge change in `catalog/search.rs` (section 3) and its tests. Not wired yet. | `cargo test` |
| 4 | **Switch.** Wire `WingetCliSource` in `enabled_sources()`. Unwire `WingetRepoSource` and add its module comment. Migration 0004. `UPSERT_SUMMARY` keeps a stored publisher and description. `Catalog::cached_detail_any_age()` and its use in `start_install` (`services/install.rs`). SPEC §0 hard rule 2 and backlog edits. **Owner gate:** the owner confirms the change of SPEC §0 hard rule 2 before this step merges. | `cargo test` |
| 5 | **UI.** Detail loading state and its text. The card hides an empty publisher. The error text for `agreements_required` on a read-only call. README limit. | `cd ../ui && npm run check && npm run build` |
| 6 | **Real-winget check.** `tests/winget_catalog_smoke.rs`, `#[ignore]`, read-only. It runs search "vlc", a no-match search and a first `show`, with no agreement flag, and prints the times. A CI job runs it with `--ignored` on `windows-latest`; it is a manual gate before step 4 (see below), not a CI gate. Record the times and the agreement result in ADR 0001. | `cargo test --test winget_catalog_smoke -- --ignored` (CI only) |
| 7 | **Recorded version** (SPEC §6.7 gap row). After a successful install, read the version from `winget list` and keep the target as the fallback. Optional for #20; it can move to WP6. | `cargo test --test install_loop` |

Steps 1 and 3 add code that nothing calls. Step 2 changes behavior: the winget floor moves
from 1.6 to 1.29, and this affects installs. Step 4 closes F1 (#20) and #16. Step 6 is a
manual gate before step 4: a person runs the check and reads its result. It is not a CI gate.
It must pass before step 4 merges, because it checks ADR 0001 conditions 2 and 3 and the agreement
point of section 6. If step 6 cannot run before step 4, the owner decides the merge.
