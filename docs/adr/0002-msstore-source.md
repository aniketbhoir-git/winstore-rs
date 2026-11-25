# ADR 0002 — msstore source

- **Status:** proposed — recommended option A; GTAVI mode: the orchestrator accepts it for
  planning, the owner may reverse.
- **Date:** 2026-11-25
- **Settles:** review action A6 (`docs/ISSUE-BACKLOG.md` F3). Feeds issue #8 (SPEC §7, WP7) and
  review action A17 (`is_paid`).
- **Depends on:** F4, issue #45, built in PR #46 (branch `impl/45--consent-case-b`, not merged on
  2026-11-25).
- **Measured on:** no new measurement. The facts come from the spike of 2026-11-13, the research
  notes of 2026-11-25 and Microsoft Learn (read on 2026-11-25).

## Problem

SPEC §1.4 goal 3 is "Show Microsoft Store items; a paid item gets the browser hand-off only".
SPEC §0 says that the app reaches the `msstore` winget source only through the winget CLI. That
rule was written before ADR 0001 and before the spike. Issue #8 must not start until the rule is
confirmed or replaced (review action A6).

Four facts make the decision necessary:

1. `msstore` is a REST source (`https://storeedgefd.dsx.mp.microsoft.com/v9.0`). Its REST
   endpoints answer a plain HTTPS call with no account. A direct call is thus possible, but the
   Store service does not publish them as an interface.
2. `msstore` has a source agreement, "Terms of Transaction"
   (`https://aka.ms/microsoft-store-terms-of-transaction`), and sends the market (the 2-letter
   country code) to the service. The agreement has a label and a link, and no text (spike E1).
3. An msstore item can be paid. The app must never install a paid item and must never touch
   payment, account or licensing logic (CLAUDE.md, SPEC §0 hard rule 1).
4. The catalog design (`docs/designs/catalog-winget-cli.md`) always names `--source winget`
   and leaves `msstore` out. A second source adds a merge problem in search.

## Constraints

- Public, documented interfaces only. No scraping. No account sign-in.
- No payment, account, licensing or entitlement logic. A paid item shows only "Buy in Microsoft
  Store", which opens the official page in the default browser.
- No telemetry. The app calls only the catalogs that the user turned on (hard rule 5).
- Hard rule 4 and SPEC §6.6: the app never accepts an agreement that the user did not see, and
  it stores no consent. An agreement flag goes to winget for one call, directly after a user
  "Accept".
- `msstore` is off by default. When it fails, the adapter returns `SourceUnavailable`.
- Automated tests use recorded output. No test calls the real winget, except the read-only
  smoke tests on a GitHub-hosted runner.

## Options

### A. winget CLI with `--source msstore`, through the existing executor

- **Search:** `winget search --query <text> --source msstore --count <n>
  --disable-interactivity`. The same table parser as the `winget` source reads the rows (Name,
  Id, Version). The Store service ranks the rows. Version is often "Unknown" in a search row.
- **Detail:** `winget show --id <id> --exact --source msstore --disable-interactivity`. Spike
  E2 shows name, publisher, description, license URL, and an `Agreements:` block with
  `Pricing: Free`, the Terms of Transaction link and the Store license link.
- **Install:** `winget install --id <id> --exact --source msstore`. The executor already maps
  `SourceKind::MsStore` to `msstore` (`executor/winget_cli.rs`). winget does the deployment.
- **Agreements:** winget reports the source agreement with exit `0x8A150046` and prints the
  label and the URL to stdout. F4 (PR #46) parses this block and binds a consent token to the
  package, version, winget source and kind.
- **Interface rule:** passes. The winget CLI and its `--source`, `--accept-source-agreements`
  and `--disable-interactivity` options are on Microsoft Learn (`winget source`, `winget
  show`). The `msstore` source is a named default source on Learn.
- **Size of the change:** small. A second instance of the CLI catalog adapter with a different
  source name, a price rule and the read-only consent step below.

### B. The storeedgefd REST endpoints directly (`/information`, `/manifestSearch`)

- The research of 2026-11-25 called `POST .../v9.0/manifestSearch` and got correct rows for
  "vlc". The REST contract of `winget-cli-restsource` is public, but it is the contract for
  REST sources that other people host.
- Microsoft does not publish the Store instance as an interface for third-party clients.
  Nothing promises that it stays open, stable or free of new parameters. It already requires
  `market` as a query field.
- The app would send the market to a Microsoft service with no winget in between, and it would
  have to show and enforce the Terms of Transaction itself.
- **Fails the rule "public, documented interfaces only".** It is the same reason that ADR 0001
  gives for its option B. The research proposal "Query `msstore` over REST" is not adopted.

### C. `Windows.Services.Store` or the Windows Deployment API from an unpackaged app

- `Windows.Services.Store.StoreContext` is a documented API, but its purpose is purchases,
  add-ons, licenses and trials of the calling app. Each of these is payment or licensing logic.
  **Fails the hard rule "no payment, account or licensing logic".**
- `StoreContext` needs package identity for most calls. Store is an unpackaged app. Access is
  not verified.
- `Windows.Management.Deployment.PackageManager` installs an MSIX package from a file or a URL.
  It does not search the Store catalog and does not resolve a Store product id. It does not
  solve the problem. It stays deferred as WP11 (SPEC §2.1).
- No catalog API in this group gives search with a documented contract for an unpackaged app.

### D. Defer msstore

- Keep the `msstore` source off and unbuilt. The detail page shows no Store items.
- No new risk, no new code.
- SPEC §1.4 goal 3 stays unmet. Issue #8 stays open with no plan. The Buy-in-Store button and
  `open_store_page` (built) have no data that uses them.

## Comparison

| | A CLI `--source msstore` | B storeedgefd REST | C Store / Deployment API | D Defer |
|---|---|---|---|---|
| Follows the interface rule | Yes | No | Partly (API documented) | n/a |
| Follows "no payment/licensing logic" | Yes | Yes | No (`StoreContext`) | Yes |
| Search of the Store catalog | Yes, Store ranking | Yes | No | No |
| Installs a free item | Yes, through winget | No (needs winget anyway) | No | No |
| Source agreement handled by | winget + F4 consent | The app (new logic) | n/a | n/a |
| Needs winget | Yes | No for search | No | n/a |
| New code | Small | Medium, plus agreement logic | Large, plus bindings | None |
| Risk that stays | Text format, slow REST calls, price line | Service change, rule break | Rule break | Goal 3 unmet |

## Recommendation

Option A, with the conditions below. SPEC §0 already names it. Option B and option C each fail
a hard rule. Option D leaves goal 3 unmet and gives no benefit that option A cannot also give
while `msstore` stays off by default.

What is wrong with option A comes first:

1. **Every msstore call is a network call through winget.** The `winget` source reads a local
   index; `msstore` calls the Store service for each search and each `show`. Search is slower
   and can fail when the network fails. Mitigation: the same time limits as the catalog design
   (15 s search, 60 s detail); on a time-out the adapter returns `SourceUnavailable`, and the
   `winget` rows still show.
2. **The price line is not a documented field.** See "Paid apps". Mitigation: fail safe. Any
   doubt gives `is_paid = true`, and the item gets the hand-off only.
3. **The source agreement blocks read-only calls.** On a machine that never accepted it, the
   first search fails with `0x8A150046`. See "msstore source agreement".
4. **The market goes to Microsoft.** winget sends it, not the app. The consent dialog must show
   the market note that winget prints (spike E1).

### msstore source agreement (F4, issue #45)

- Install and uninstall: the case (b) consent of F4 applies with no change. PR #46 binds the
  consent token to the package id, the version, the winget source (`msstore`) and the kind
  (`source` or `package`). An msstore install can need both kinds in one attempt.
- Read-only calls (search, detail): SPEC §6.6, "Read-only calls", requires a step before code.
  The step for `msstore`:
  1. Each read-only call names `--source msstore` and `--disable-interactivity`, and passes no
     agreement flag.
  2. When the call ends with `0x8A150046`, the adapter returns `agreements_required` with kind
     `source`, the label, the URL and the market note from stdout, and a consent token bound to
     the winget source `msstore`, the kind `source`, and no package.
  3. The UI shows the F4 consent dialog. After "Accept", the UI repeats that one call with the
     token. The core passes `--accept-source-agreements` to that one call only.
  4. winget then keeps the acceptance itself (spike E6). Later calls pass no flag. The app
     stores nothing. When the agreement identifier changes, winget asks again and step 2 runs
     again.
  5. The `winget` source rows never wait for this step. A blocked `msstore` call gives an empty
     msstore section with a "Review terms" action, not a failed search.
- `guard_args()` refuses agreement flags today. Step 3 needs one exception: the source flag on
  a read-only `msstore` call that carries a valid token. This is a SPEC §6.6 change.
- Rejected: the research note "Save the acceptance with the agreement identifier"
  (`docs/research/2026-11-25-install-progress-and-agreements.md`). SPEC §6.6 forbids a stored
  consent.

### Paid apps (review action A17)

- The rule of SPEC §6.4 stays: `is_paid = true` unless the data shows that the item is free.
- The only free signal is the line `Pricing: Free` in the `Agreements:` block of `winget show`
  (spike E2). The Learn page for `winget show` does not document the block or its fields. The
  value comes from the source data, and winget prints it as data. The adapter therefore treats
  it as a hint that can only lower risk:
  - exactly `Pricing: Free` (after trim, case as printed) gives `is_paid = false`;
  - any other value, a missing line, a missing block or a parse failure gives
    `is_paid = true`.
- A search row has no price. A search card from `msstore` therefore starts with
  `is_paid = true` and has no Install button. The card opens the detail page; the detail read
  sets the value. This changes the WP7 acceptance text ("a search returns a free Store item as a
  `PackageSummary` with `is_paid = false`"). See "Open questions for the owner".
- A paid item, or an item with no price data, shows only "Buy in Microsoft Store". No install
  path, no trial, no price text. The app never reads or shows the price itself.

### Hand-off URL

| Form | Documented | Opens |
|---|---|---|
| `https://apps.microsoft.com/detail/<id>` | Named in SPEC §0 rule 1 and built in `store_page_url()`. A Learn page that defines this form was not found on 2026-11-25. | The default browser. |
| `ms-windows-store://pdp/?ProductId=<id>` | Yes: Learn, "Launch the Microsoft Store app" (`learn.microsoft.com/en-us/windows/apps/develop/launch/launch-store-app`). It names the product id as the recommended key. | The Store app, not the browser. |

Keep the HTTPS form. CLAUDE.md and hard rule 1 require the browser. The HTTPS page also works on
a machine with no Store app. The `ms-windows-store:` form stays out; it would change hard rule 1.

### msstore id format

- A Store product id is ASCII letters and digits. The records show two forms: 12 characters
  that start with `9`, for example `9NBLGGH4NNS1` (packaged app), and 14 characters that start
  with `XP`, for example `XPDM1ZW6815MQM` (a Win32 app in the Store). The length is thus not
  fixed. No Learn page that defines the format was read.
- `PackageId` is `ms_store/<product id>`. `store_product_id` (SPEC §4.2) holds the same value.
- `store_page_url()` accepts 1 to 32 ASCII letters and digits. This covers both forms. The
  adapter rejects a row with any other id; it never builds a URL from an unchecked id.
- The id goes to winget as one argument with `--exact`. No shell runs.

### Search across two sources (`catalog/search.rs`)

- `merge()` joins the sources in priority order and drops a later row only when the dedupe key
  (name and publisher) matches. A winget search row has no publisher, so `dedupe_key()` is
  `None` and the row is never dropped. The same holds for `msstore` search rows.
- Result: "vlc" shows `VideoLAN.VLC` (winget) and `XPDM1ZW6815MQM` (msstore) as two cards. This
  is correct. They are different packages with different installers.
- Priority: `winget` first, `msstore` second (SPEC §5.3 "planned" rule). The merged order thus
  keeps winget's own ranking at the top. Each card shows its `SourceBadge`.
- The UI shows the msstore rows as a separate, labelled section, as the research proposes. No
  new dedupe rule is needed. A name-only dedupe across sources is rejected: it would hide a
  Store item behind a community item of the same name.
- A failed or blocked `msstore` call never fails the search. The coordinator returns the
  `winget` rows and a per-source status.

### Runner smoke (`.github/workflows/catalog-smoke.yml`)

The smoke test may never accept an agreement. On a fresh GitHub-hosted runner it can still
measure:

1. `winget source list` names `msstore` with its URL.
2. `winget search --query vlc --source msstore --count 5 --disable-interactivity` ends with
   `0x8A150046` and does not wait for input. The test records the exit code, the time to the
   exit, and the raw stdout.
3. The raw stdout holds the agreement block: the title line, the label "Terms of Transaction",
   the URL and the market note. This is the fixture for the F4 parser on a real `msstore`
   source agreement.
4. The same for `winget show --id 9NBLGGH4NNS1 --exact --source msstore`.
5. The `winget` source results of the same job are not changed by the blocked `msstore` call.

It cannot measure: real msstore search rows, the `Pricing` line, the time of a successful
msstore call, or an install. Those fixtures come from a machine that already accepted the
agreement (the spike machine did), recorded with read-only calls and no flag.

## Consequences if option A is accepted

- `sources/msstore.rs` is a second CLI catalog adapter with `--source msstore`. It reuses the
  table and key/value parsers of ADR 0001.
- SPEC §0 stays as written. SPEC §6.3 step 2 can now say: a read-only call names one winget
  source, `winget` or `msstore`, never none.
- SPEC §6.6 "Read-only calls" gets the `msstore` step above, and `guard_args()` gets its one
  exception.
- SPEC §6.4 step 3 and review action A17 get the price rule above.
- The search response carries a status for each source, so a blocked `msstore` section does not
  hide the `winget` rows.
- No new dependency. No REST client for the Store service.

## Conditions

1. PR #46 (F4) merges before the code of #8. The msstore install needs its consent tokens.
2. The SPEC §6.6 change for read-only `msstore` calls (token, one-call flag, `guard_args()`
   exception) is written and reviewed before code.
3. The runner smoke records the `0x8A150046` output for search and show before the parser for
   that block merges.
4. A recorded `winget show` from a machine that accepted the agreement covers one free and one
   paid item. If no paid item can be recorded, every msstore item gets the hand-off only until
   one is.
5. The adapter never parses the price except for the exact `Pricing: Free` rule.

## Open questions for the owner

1. Accept that msstore search cards have no Install button and that the detail decides
   `is_paid`? This changes the WP7 acceptance text.
2. Accept the one `guard_args()` exception (the source flag on one read-only `msstore` call
   after "Accept")? The alternative is a message that tells the user to accept in a terminal,
   as the catalog design does for the `winget` source.
3. Keep the HTTPS hand-off only, or also allow `ms-windows-store://pdp/?ProductId=` (documented)?
   This ADR keeps HTTPS only.

## Not verified

- A Learn page that defines `https://apps.microsoft.com/detail/<id>`.
- That the `Pricing` field exists for every msstore item, and its values for a paid item or a
  trial.
- The msstore output on a non-English Windows.
- That a fresh GitHub-hosted runner reaches the Store service and gets `0x8A150046` and not a
  network error.
- The time of a successful msstore search and `show`.
- Whether `winget uninstall --source msstore` needs the source agreement (spike question 3
  says it takes the flag).
