# Search ranking: winget, UniGetUI, Microsoft Store, winget.run

Research date: 2026-11-25. Context: bug #20 (search for "vlc" returns unrelated apps) and issue #16.
All code links point to the `master` (or `main`) branch of each repository on that date.

## 1. How `winget search` ranks results

### The query

- `winget search <text>` builds one request with `MatchType::Substring`. With `--exact` it uses `MatchType::Exact`.
  Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/Workflows/WorkflowBase.cpp (function `SearchSourceForMany`).
- The Microsoft Learn page says the search is a "case-insensitive substring match against the package name, ID, and moniker".
  Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/search
- The code does more than the page says. It also searches the Command and Tag fields (see below).

### The match types (ordered)

The enum is declared in order of preference. The source comment says: "The values must be declared in order of preference in search results."
Order: `Exact, CaseInsensitive, StartsWith, Fuzzy, Substring, FuzzySubstring, Wildcard`.
Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/Public/winget/RepositorySearch.h

### The fields (ordered)

Order: `Id, Name, Moniker, Command, Tag, PackageFamilyName, ProductCode, UpgradeCode, NormalizedNameAndPublisher, Market`.
Source: same file (RepositorySearch.h).

### The expansion of one query

A request for `Substring` is expanded to two passes: first `CaseInsensitive`, then `Substring`.
Source: `GetDefaultMatchTypeOrder` in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/Microsoft/Schema/ISQLiteIndex.cpp

The index then runs this loop (schema 2.0 and later, which is the current index):

1. Exact match on the folded PackageFamilyName, ProductCode and UpgradeCode.
2. For each match type in the expanded list, for each field in `Id, Name, Moniker, Command, Tag`: run the search.

Source: `PerformQuerySearch` in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/Microsoft/Schema/2_0/Interface_2_0.cpp

Each search call gets the next sort number. A package that matches several times keeps its lowest number. The result is ordered by that number.
Source: `SearchOnField` (`m_sortOrdinalValue++`) and `GetSearchResults` (group by id, order by sort) in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/Microsoft/Schema/1_0/SearchResultsTable_1_0.cpp

So the rank of a package is the first pass that finds it:

| Pass | Match type | Field |
|---|---|---|
| 1 | exact (PFN, product code, upgrade code) | system references |
| 2 | case-insensitive equal | Id |
| 3 | case-insensitive equal | Name |
| 4 | case-insensitive equal | Moniker |
| 5 | case-insensitive equal | Command |
| 6 | case-insensitive equal | Tag |
| 7 to 11 | substring | Id, Name, Moniker, Command, Tag |

### Merging several sources

When a query runs on several sources, the composite source sorts the combined list with a stable sort. The key is match type first, then field. Equal keys keep source order.
Source: `ResultMatchComparator` and `SortResultMatches` in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/CompositeSource.cpp

There is no popularity score, no download count and no text relevance score. The ranking is a fixed ladder.

### Check against the real index (2026-11-25)

I downloaded `https://cdn.winget.microsoft.com/cache/source2.msix` (4,071,168 bytes) and read `Public/index.db`.
It is schema 2.1 with 16,504 rows in `packages`. I ran the ladder above for the query "vlc" in a throw-away script.
Result: 10 matches. The first is `VideoLAN.VLC` ("VLC media player", latest 3.0.24) because its moniker equals "vlc".
This meets the F1 acceptance rule (VLC in the first 3 results). It needs no network service and no ranking tuning.
My script is a simplified model of the C++ code (it does not fold or normalize text). Treat the exact order of the tail as approximate.

## 2. How UniGetUI searches

- UniGetUI runs the search per package manager and shows the results in one list. The loader caps each manager at 100 results to avoid a freeze.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.PackageLoader/DiscoverablePackagesLoader.cs
- For WinGet it uses the COM API. For each catalog and each of the fields `Name, Id, Moniker` it sends a filter with `PackageFieldMatchOption.ContainsCaseInsensitive`, then removes duplicates by (catalog, id).
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/ClientHelpers/NativeWinGetHelper.cs (`SearchFields`, `FindPackages_UnSafe`, `SelectUniqueSearchMatches`)
- If COM is not usable, it falls back to the CLI: `winget search "<query>" ... --count` and caps at 100 results. The code comment says `winget search a` returns about 12,000 results.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/ClientHelpers/WinGetCliHelper.cs
- The page has search modes: Both (name or id), Name, Id, Exact, Similar. The filter is applied on the client after the results arrive.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.Avalonia/ViewModels/SoftwarePages/PackagesPageViewModel.cs
- I found no cross-manager relevance score. The default list order is a column sort (default: Name, ascending). This is a negative finding from reading the loader and the page view model, not a proof.
  Source: same file (`SortFieldIndex`, default name ascending).

Lesson: UniGetUI trusts winget's own matching and adds a client-side name/id filter and a result cap. It does not build its own index.

## 3. How the Microsoft Store source searches

- winget has a built-in source named `msstore` at `https://storeedgefd.dsx.mp.microsoft.com/v9.0` with identifier `StoreEdgeFD`. It is a REST source.
  Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/SourceList.cpp
- The REST contract has `POST /manifestSearch`. Match types: Exact, CaseInsensitive, StartsWith, Substring, Wildcard, Fuzzy, FuzzySubstring. Keyword is at most 255 characters.
  Source: https://github.com/microsoft/winget-cli-restsource/blob/main/documentation/WinGet-1.9.0.yaml
- The Store server decides its own ranking. I found no public document that describes it.
- Live check on 2026-11-25. `GET https://storeedgefd.dsx.mp.microsoft.com/v9.0/information` returned 200. It lists server versions 1.0.0, 1.1.0, 1.6.0 and says `RequiredQueryParameters: ["market"]`. It also lists one source agreement: "Terms of Transaction" (https://aka.ms/microsoft-store-terms-of-transaction).
- Live check. `POST .../v9.0/manifestSearch` with `{"Query":{"KeyWord":"vlc","MatchType":"Substring"},"MaximumResults":5}` returned `XPDM1ZW6815MQM` "VLC" (VideoLAN) and `9NBLGGH4VVNH` "VLC UWP". Both are correct. The version is "Unknown" in the search row.
- The Store source asks the user to accept terms, and it declares a market field. Treat it as a second, optional source. Do not mix it into the community list without a source label.

## 4. winget.run: what it was and its state

- `winget.run` was a community REST site and web app. Its repositories are archived: `winget-run/api` (last push 2023-12-10), `winget-run/web` (2023-03-04), `winget-run/docs` (2023-01-07).
  Sources: https://github.com/winget-run/api , https://github.com/winget-run/web , https://github.com/winget-run/docs (GitHub API field `archived: true`).
- The API host still answers. On 2026-11-25 `GET https://api.winget.run/v2/packages?query=vlc&take=2` returned HTTP 200 with `FlockFZ.Flock`, `UpdatedAt` 2023-03-16. This matches ADR 0001: the data is frozen and the match is not by name.
  Source: https://api.winget.run/v2/packages?query=vlc&take=2 (live response).
- Conclusion: no maintainer, no new data since March 2023. Do not depend on it. This is the cause of bug #20.

## Features worth building

- **Local ranked search over the official winget index.**
  Download `source2.msix`, read `index.db`, and rank with winget's own ladder (equal before substring; Id, Name, Moniker, Command, Tag).
  This fixes #20 with no live service, and tests can use a small fixture index.
- **Show why a result matched.**
  Show a small badge: "ID match", "Name", "Tag", "Command". Users see why an app is in the list and trust the order.
- **Search modes: Name, Id, Exact.**
  Copy UniGetUI's modes. Power users get exact lookups; normal users keep the default.
- **Result cap with "show more".**
  Cap at 100 results per source. A one-letter query returns about 12,000 rows in winget, so a cap protects the UI.
- **Optional Microsoft Store results as a separate section.**
  Query `msstore` over REST. Label the source and show its terms link. Keep it off by default.
- **Golden search tests.**
  Test "vlc" (VideoLAN.VLC in the first 3) and a no-match string (empty list) against a fixture index with no network.
