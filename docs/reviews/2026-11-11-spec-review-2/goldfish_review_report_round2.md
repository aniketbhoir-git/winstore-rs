# Resolution of Stage 7 round 2 findings

- **Status:** proposed, waits for maintainer approval
- **Source:** `critic_report_round2.md` (R1 to R20), `docs/SPEC.md` at commit `e51297c`
- **Date:** 2026-11-12
- **Counts:** 18 PROPOSE FIX, 1 ESCALATE (R9), 1 split (R20: part fix, part decline)

Checks made in the main context before the proposals:

- **R7 is correct.** The header comment of `services/install.rs` gives a cancelled uninstall
  `Installed` (or `Pinned`) with no check for a recorded version.
- **R2 is correct, with a lower chance than the reviewer assumed.** `tauri.conf.json` sets no
  CSP. The UI has no `{@html}`, so Svelte escapes catalog text today. Nothing in the spec
  keeps it that way.
- **R16:** `settle` moves a row to `Pinned` when the pin flag is set. Nothing sets the flag.
  Lines 267 and 541 are both true; the text of line 267 is the unclear one.

## Proposals

| Id | Decision | Proposed change to `docs/SPEC.md` |
|---|---|---|
| R1 | PROPOSE FIX | §6.6: define **attempt** = the chain of operations that consent retries link. An *operation* stays one winget run with one `operation_id`. A flag is valid for one operation. The UI sends, with each retry, the kinds that the user accepted in the attempt. The dialog opens two times at most for one attempt. |
| R2 | PROPOSE FIX | §6.6, new rule "Trust boundary": the webview is the only origin of a consent, so WP5 must (1) set a CSP with `script-src 'self'` and no inline script, and (2) show catalog text as text only, never as HTML. Add a test for (2) and a risk row in §2.3. The alternative "the core opens a native dialog" goes into the §6.6 alternatives table as open, for the spike to judge. |
| R3 | PROPOSE FIX | Status note: add the mark *built on branch* = code on `impl/6--install-loop`, not on `main`. For WP5 that branch is the reference. Mark §6.2, §6.4, §6.7 and the WP5 rows of §5.4 with it. State: the spike and §6.6 are the WP5 work that remains, and PR #19 does not merge before them. |
| R4 | PROPOSE FIX | §2.2: one line of reason for each rejected or open option, taken from ADR 0001 and `docs/STRATEGY.md`. §6.6: an alternatives table: a visible winget prompt (rejected: needs a console window and stops the progress parser); a stored consent (rejected: round 1 K1); the config flag (rejected: hard rule 4); the COM API (open, §2.2); a core-owned native dialog (open, R2). |
| R5 | PROPOSE FIX | §6.6: the read-only paragraph covers each read-only winget call. If ADR 0001 picks option A, that includes search, `get_app_detail` and the detail read in `start_install`. Add this as a condition in ADR 0001 also. |
| R6 | PROPOSE FIX | §2.3: a risk row for the slow first `winget show` (21 to 23 s on one machine). §6.2 step 1: `start_install` uses the cached detail when one exists. The time limit is set by the ADR 0001 work, not here. |
| R7 | PROPOSE FIX | §6.7 end table, *planned*: a cancelled uninstall goes to the rest state, not always to `Installed`. New gap row, owner WP5. |
| R8 | PROPOSE FIX | §6.7 agreement row and §6.6 step 2: the ledger goes to the **rest state**. Note: an earlier `Failed` becomes `NotInstalled`. Each edge that this needs is legal today. Change the WP5 test text to match. |
| R9 | **ESCALATE** | Hard rule 5 is a maintainer rule. See "Needs the maintainer" below. |
| R10 | PROPOSE FIX | A short "Problem" paragraph before §1, with the user problem and the users from `docs/STRATEGY.md` §1. No new claim. |
| R11 | PROPOSE FIX | §6.4: `is_paid` stays a boolean. The adapter sets `is_paid = true` unless the catalog data shows that the item is free. "Unknown" is then not a stored value. |
| R12 | PROPOSE FIX | Move the four terms of §6.7 (ledger, active, rest state, busy states) to §0, with "attempt" from R1. §6.7 points to §0. |
| R13 | PROPOSE FIX | §4.1: add the operation kind enum with the names that the code uses. §4.2: add the install options (version, scope, consent) and the consent shape (accepted kinds, shown version). Write "operation kind" where the field means the kind. |
| R14 | PROPOSE FIX | §5.4: add the planned `options.consent` argument to `start_install`, `start_uninstall` and `check_updates`, marked *planned (§6.6)*. §6.3: `update_all` stops before the first package and returns `agreements_required` when the source agreement is missing. §6.6: replace "a library list" with "the WP6 library list follows the same rule". |
| R15 | PROPOSE FIX | Write "catalog source" for a `CatalogSource` adapter and "winget source" for a source of winget. "Source agreement" becomes "winget source agreement". |
| R16 | PROPOSE FIX | §4.1: "no code path reaches `UpdateAvailable` or `Pinned` before WP6, because nothing sets the pin flag and nothing writes the scan result". Gap row A10: add "rename the struct `UpdateAvailable`" to the WP6 fix. No rename of built code now. |
| R17 | PROPOSE FIX | §6.6 step 4: when the shown version is not available, the operation ends with `package_not_found`, and the message names the version. The attempt ends; a new attempt starts with no consent. |
| R18 | PROPOSE FIX | One name for the actor: `services/install.rs` settles the ledger and writes `operation_log`. I check the code first and write what it does. "Warn log row" becomes "Warn row in `operation_log`". |
| R19 | PROPOSE FIX | Seven one-sentence fixes: "expired" for a cache row past its time limit; one executor rule (by `SourceKind`); the HTTPS hand-off URL is the one to use, after a check of `open_store_page`; rule 4 lists install, update, uninstall and pin in place of "destructive-or-silent"; "v1" becomes "before WP7" or "before WP9" in each place; the id format after a code check; "first use of the core" becomes "the first IPC command that builds the core". |
| R20 | PROPOSE FIX (part) | Owner WP5 inline in §6.3 step 2. WP marks on the UI components after a code check. One line of meaning at the first use of each A-id. A note that `OperationStage` values are the wire strings. `is_auto` listed in §4.2 as reserved. The merged search card keeps the id of the catalog source with the highest priority. |
| R20 | DECLINE (part) | Move of Goals to §1.1 (renumbers each cross-reference for no change of meaning). The wording list of style-2-18, design-2-10 and design-2-12: these go to the A25 cleanup, the same as round 1 Issue 26. |

## Needs the maintainer

**R9 — hard rule 5 against remote images.** Rule 5 says the app makes network calls only to
the enabled catalogs and through winget. The app loads icons and screenshots from the hosts
that the catalog data names. Two options:

- **(a) Reword rule 5.** Permit HTTPS image loads from hosts that an enabled catalog names,
  until the WP8 image decision. Recommended: it describes what the app does, and no image
  breaks.
- **(b) Keep rule 5.** Limit images to the hosts of the enabled catalogs with a CSP `img-src`
  list. Most icons then do not load, because publishers host them.

## Still open from round 1

- Issue 21: minimum Windows version, minimum winget version, the UI when winget is absent.
- The §6.6 link fallback against hard rule 4.
- The pick in ADR 0001. R5 and R6 are written to hold for each pick.
