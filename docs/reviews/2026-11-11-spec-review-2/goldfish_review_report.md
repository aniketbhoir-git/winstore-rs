# Goldfish Annotated Review Report
n**Status: resolved** (2026-11-11: 24 fixes applied to `docs/SPEC.md`; Issue 21 escalated; Issue 26 declined)

**Target:** `docs/SPEC.md` (lines 1-454, commit `961b63d`)
**Source of findings:** `critic_report.md` (Stage 7, round 1), ids K1 to K21, N-A, N-B.
Three findings from Stages 5 and 6 in the same range are added as X1 to X3.

**Counts:** 24 PROPOSE FIX, 0 CONTEST, 1 ESCALATE, 1 DECLINE.

No finding is contested. Most of the Blocker and Major findings on §6.6 and on the status note
are in text that the main context wrote in this session, and the reviewers are right about
them.

## The one design change behind the §6.6 fixes

Issues 1, 4, 5, 6, 7 and 8 all touch §6.6. They get one rewrite, with this shape:

- **The app stores no consent.** No column in `sources`, no `accept_source_agreement` command,
  no revoke path.
- **A flag goes to winget only on the one retry that follows a user "Accept".** After that,
  winget keeps or does not keep the acceptance; the app does not care. If winget asks again
  (changed terms, new machine), the app shows the dialog again.
- **This shape is safe for each answer to the three unknown facts** (how winget stores a
  source agreement, if it asks again after a change, if it gives the text with no prompt). The
  worst case is one more dialog, never a silent acceptance.
- **A spike comes first in WP5** and its result goes into the `winget-executor` skill.

## Prioritized Findings & AI Suggestions

### [Issue 1] [`egm-design`] - Severity: Blocker — K1, stored source consent

*   **Affected Scope:** lines 52-53, 334-342, 348-350, 363 (§0 rule 4, §6.6)
*   **Discrepancy Description:** the `sources` table stores only the time of consent; each
    later operation passes the source agreement flag.
*   **Rationale:** if winget asks again when terms change, the stored consent stops the
    prompt. That is silent acceptance. The revoke path is deferred.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** the reviewer did not verify the winget behavior, and it does not
    need to be verified: a design that stores nothing is correct in each case. Stage 5 finding
    Z2 reaches the same result from the other side (the storage is not necessary).
*   ***Proposed Edit:***
    *   Location: §6.6, the kind/consent/storage table, flow steps 1, 4 and 6, and the list
        "Changes that WP5 owns".
    *   Change: source agreement row becomes "Consent: each time winget asks. Storage: none in
        the app." Step 1: "An operation starts with no agreement flag." Step 4: "Accept: the UI
        calls the start command again with the consent for the kind that winget named. The
        core passes that one flag for that one operation." Delete step 6. Delete the migration
        and the `accept_source_agreement` command from the change list.

### [Issue 2] [`egm-design`, `egm-structural`] - Severity: Blocker — K2, required sections

*   **Affected Scope:** whole range; lines 3-11, 94-114
*   **Discrepancy Description:** no problem statement, goals and non-goals, alternatives or
    risks section.
*   **Rationale:** the standard fails a spec that shows one path with no rejected option.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. The fix follows the scope that the first review set (A22):
    short sections that link to STRATEGY, not copies of it. The severity stays as the
    reviewers gave it.
*   ***Proposed Edit:***
    *   Location: replace the purpose block (lines 3-11); new sections after §2 and after §6.
    *   Change: (a) a two-sentence objective. (b) "Goals and non-goals" with links to STRATEGY
        §1.2 and §1.3. (c) "Alternatives considered", one row for each open decision: catalog
        backend (link to `docs/adr/0001-community-catalog-backend.md`, status proposed); CLI
        or COM executor (the COM API gives typed progress and a real cancel; access from an
        app with no package identity is not verified; status open, decide before WP6);
        headless shape (status open, A5). Tauri against other shells: link to STRATEGY §2.3.
        (d) "Risks", only the risks this spec adds: the consent flow, the per-package lock in
        one process, the stale third-party catalog, text parsing of winget output, the
        localhost API. Each with a mitigation or an owner work package.

### [Issue 3] [`egm-design`, `egm-style`, `egm-structural`] - Severity: Blocker — K3, headless mode

*   **Affected Scope:** lines 61, 69-70, 108, 267, 320-325 (§1, §2, §5.4, §6.5)
*   **Discrepancy Description:** §6.5 describes a localhost install API with no
    authentication; axum is in the "Locked" table.
*   **Rationale:** any web page in a local browser can reach a localhost API. A5 asked for one
    gate.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid, and A5 already decided it.
*   ***Proposed Edit:***
    *   Location: §6.5; §2 table; line 61.
    *   Change: §6.5 becomes "Headless mode (WP10). Blocked until a decision record answers
        STRATEGY open question 6: hosting model, authentication in each mode, origin checks,
        consent over HTTP, winget on Windows Server. Do not implement." Move axum to a
        "Deferred, needs a decision" list under the table. Line 61: "WP10 adds ...".

### [Issue 4] [`egm-design`] - Severity: Major — K4, the dialog depends on an unknown

*   **Affected Scope:** lines 346-347, 371-373 (§6.6)
*   **Discrepancy Description:** the dialog must show agreement text; nobody knows if winget
    gives it with no prompt. The only failure outcome is "stop and report".
*   **Rationale:** the feature that closes the MVP has a possible dead end.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. Stage 5 findings Z3 and I3 say the same.
*   ***Proposed Edit:***
    *   Location: §6.6, new first subsection "Spike (first WP5 task)"; step 3; the open point.
    *   Change: the spike answers three questions on a real machine and records the answers in
        the skill: where the agreement text comes from with no prompt (`winget show` output is
        the first candidate); if winget keeps a source acceptance; if it asks again after a
        change. Fallback when no text is available: the dialog names the package, the source
        and the kind, and gives the license or agreement link from the manifest, which opens
        in the browser. **This fallback needs the maintainer's word that a link satisfies hard
        rule 4.** The text marks it as "needs maintainer approval" until then.

### [Issue 5] [`egm-design`] - Severity: Major — K5, ledger after `agreements_required`

*   **Affected Scope:** lines 343-345, 353, 378-379, 408-409, 413 (§6.6, §6.7)
*   **Discrepancy Description:** step 2 uses the "failed" rows, which give `Failed` and an
    Error log row for a first install; step 5 says a decline changes nothing.
*   **Rationale:** two sections give two results for one case. Stage 6 finding C3 is the same.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. The code on the branch settles this case as a failure, so the
    new row is a WP5 change, and the text says so.
*   ***Proposed Edit:***
    *   Location: §6.7 end table; §6.6 step 2 and step 5.
    *   Change: new row "Any operation | winget needs an agreement | `failed` | ledger as
        before the operation started | Info | `agreements_required`", marked "WP5 change; the
        branch gives `Failed` and an Error row for a first install". Step 2 points at the row.
        Step 5: "Decline: nothing more runs. The core writes one Info row."

### [Issue 6] [`egm-design`] - Severity: Major — K6, package consent is a boolean

*   **Affected Scope:** lines 337, 348-350, 364 (§6.6)
*   **Discrepancy Description:** any caller can set `accept_package_agreements = true`; the
    retry is not tied to the version or text that the user saw.
*   **Rationale:** the user can consent to one thing and get a different one.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** the second half is valid and gets a fix. The first half gets a
    smaller fix than the reviewer asked for. A one-time token held in the core does not prove
    that a person saw a dialog, because the same UI that receives the token sends it back.
    The real exposure is a second caller (the WP10 HTTP route), and Issue 3 gates that on a
    decision record that must cover "consent over HTTP". Stage 5 finding Z2 also says §6.6 is
    too heavy already.
*   ***Proposed Edit:***
    *   Location: §6.6 step 4; change list.
    *   Change: the consent on the retry carries the package version that the dialog showed.
        The core installs exactly that version and returns `agreements_required` again when
        that version is not available. Add one sentence: "No transport other than the desktop
        IPC may carry a consent until the WP10 decision record defines it."

### [Issue 7] [`egm-design`] - Severity: Major — K7, read-only and batch paths

*   **Affected Scope:** lines 264, 307-309, 341-342, 365-366 (§5.4, §6.3, §6.6)
*   **Discrepancy Description:** `check_updates` and `list_installed` can fail on a source
    agreement; the `update_all` sentence contradicts itself.
*   **Rationale:** undefined behavior on a path that exists as a command today.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. Stage 5 finding Z4 gives the smallest fix.
*   ***Proposed Edit:***
    *   Location: §6.3 step 1; §6.6; §6.7 gap table.
    *   Change: "In v1 each read-only winget call names the `winget` source. A read-only call
        never passes an agreement flag. If it ends with `agreements_required`, the command
        returns that error and the UI shows the consent dialog." Move the `update_all` line
        out of the WP5 list to a WP6 note: "`update_all` skips a package that needs a package
        agreement and records it in the summary." Gap row: "`check_updates` runs with no
        `--source` today | WP5".

### [Issue 8] [`egm-design`, `egm-structural`] - Severity: Major — K8, the skill says the opposite

*   **Affected Scope:** lines 17-18, 297-299, 358-362; `SKILL.md` lines 19, 61, 153
*   **Discrepancy Description:** the spec points to the skill for flags; the skill says to
    pass the two accept flags from the global config value.
*   **Rationale:** a reader who follows the pointer rebuilds the forbidden behavior. This is
    Stage 5 Blocker Z1.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. The skill describes the code on the branch correctly today, so
    the skill changes in the same pull request as the code, not now.
*   ***Proposed Edit:***
    *   Location: status note; §6.6 change list.
    *   Change: status note gets "Exception: for agreement flags, §6.6 overrides the skill
        until WP5 updates it." Change list gets "Update the `winget-executor` skill (hard
        rules, command lines, test list) to this section, in the same pull request."

### [Issue 9] [`egm-structural`, `egm-style`, `egm-design`] - Severity: Major — K18, status framing

*   **Affected Scope:** lines 13-16, 249, 320, 327, 375, 288-325
*   **Discrepancy Description:** the note calls WP0 to WP5 "the code that exists"; §6.6 is
    WP5 and not built; WP5 is on a branch; markers have three forms.
*   **Rationale:** the reader cannot tell contract from plan. Stage 6 finding C1 is the same.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid; the main context wrote the note and §6.6 in one edit and did
    not check one against the other.
*   ***Proposed Edit:***
    *   Location: status note; each §6 subsection heading.
    *   Change: "WP0 to WP4 are on `main`. WP5 is on branch `impl/6--install-loop` (PR #19).
        For those, a section marked *built* is the contract: where it and the code disagree,
        the code is the fact; report it and correct this document. A section marked *planned*
        is the plan and overrides the code." One marker per §6 subsection, in one format:
        `(WPn, built)` or `(WPn, planned)`. §6.2 step 2 is marked as planned.

### [Issue 10] [`egm-structural`, `egm-style`, `egm-design`] - Severity: Major — K19, §6.1 against §5.4

*   **Affected Scope:** lines 257, 288-292
*   **Discrepancy Description:** §6.1 says `search_sources(query, filters)`; §5.4 says
    `search_id, query` plus events.
*   **Rationale:** two signatures for one command. Stage 6 finding C4.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** §5.4 is from the code; §6.1 is old text.
*   ***Proposed Edit:***
    *   Location: §6.1 steps 1 to 3.
    *   Change: `search_sources(search_id, query)`; the result returns from the call and also
        arrives as `store:search-results` or `store:search-failed` with the same `search_id`,
        so the UI can drop a late answer. Remove `filters`. Mark the per-source cache step
        "pending ADR 0001".

### [Issue 11] [`egm-structural`, `egm-style`] - Severity: Major — K20, the §3 tree

*   **Affected Scope:** lines 120-192
*   **Discrepancy Description:** `services/search.rs` is "not planned" and in the tree;
    `ipc/tests.rs` is nowhere; `CatalogSource` is in the wrong file.
*   **Rationale:** the tree and the list next to it disagree. Stage 5 finding Z9.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. The smaller fix (correct the tree) is used here; the larger
    one (a file table per work package, A21) belongs to the Stage 9 range.
*   ***Proposed Edit:***
    *   Location: §3 tree and list.
    *   Change: remove `services/search.rs`. Add `executor/winget_parse.rs`, `db/repo/`,
        `ipc/tests.rs`, `ipc/test_support.rs`. Move the `CatalogSource` comment to
        `catalog/mod.rs`. Mark each path that is not built with its work package in the tree
        comment, and delete the "not created yet" bullet that repeats it.

### [Issue 12] [`egm-style`, `egm-design`, `egm-structural`] - Severity: Major — K21, framing and claims

*   **Affected Scope:** lines 1, 3-11, 23, 50, 94-114, 196, 204, 227, 286
*   **Discrepancy Description:** the purpose block defends the document; headings give orders
    to a code generator; rule 3 forbids the label that §6.4 requires; the "Locked" table holds
    choices that are open; "far safer" and "fast" have no evidence.
*   **Rationale:** wrong reader, and a rule that removes a required button text.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid; all are in A25.
*   ***Proposed Edit:***
    *   Location: title, purpose block, headings of §0, §4, §4.2, §5, §6; rule 3; §2.
    *   Change: title "Store — implementation specification". Remove the bracket instructions
        from the headings. Rule 3: "The product name and branding are original. Do not use
        Microsoft logos or artwork. The text label 'Buy in Microsoft Store' on the hand-off
        button is permitted." §2: "Svelte + TypeScript"; rename the table "Technology
        decisions"; Scoop, Chocolatey, axum and the `PackageManager` executor go to the
        deferred list of Issue 3. Rationale: "winget handles installer execution, hash checks,
        agreements, dependencies and MSIX deployment; the app wraps it and does not write
        installer logic."

### [Issue 13] [`egm-design`] - Severity: Major — K14, `winget.run` as a sanctioned interface

*   **Affected Scope:** lines 36, 156, 288-292
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid; F1 proves it.
*   ***Proposed Edit:***
    *   Location: §0 term list; §3 comment; §2 rationale.
    *   Change: line 36 becomes "The community catalog backend named in
        `docs/adr/0001-community-catalog-backend.md`. Until that record is accepted, the code
        reads `https://api.winget.run`, which returns stale rows (F1)." Delete the sentence
        that justifies a separate HTTP catalog layer by speed.

### [Issue 14] [`egm-design`, `egm-style`] - Severity: Major — K15, msstore as a REST service

*   **Affected Scope:** lines 32, 48-49, 157
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** A6 decided this.
*   ***Proposed Edit:***
    *   Location: line 32; tree comment at line 157.
    *   Change: "winget `msstore` source, reached only through the winget CLI. Off by default;
        the user turns it on. On failure: `SourceUnavailable`."

### [Issue 15] [`egm-design`] - Severity: Major — K17, no-telemetry rule

*   **Affected Scope:** lines 42-53, 206
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** the rule is in `CLAUDE.md` and not in §0, which is the part that the
    backlog tells contributors to give to a model.
*   ***Proposed Edit:***
    *   Location: §0 hard rules; risks section.
    *   Change: rule 5: "No telemetry. The app makes network calls only to the catalogs that
        the user turned on, and through winget." Risk row: "Remote icons and screenshots send
        the user's IP address to other hosts. Mitigation: a content security policy that
        permits HTTPS images only; the image proxy decision is in WP8."

### [Issue 16] [`egm-design`] - Severity: Major — K9, the lock

*   **Affected Scope:** lines 79-80, 311, 395, 433-438
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid as a statement of limits. The queue and the single-instance
    rule are code work for WP6 (A11), not for the WP5 close.
*   ***Proposed Edit:***
    *   Location: §1.2; §6.3 step 3; §6.7 gap table.
    *   Change: §1.2: "The lock is per package and lives in one process. It does not stop two
        installs of different packages, and it does not cover a second app instance." §6.3:
        "sequentially", with no "configurable". Gap rows: "One queue for each operation that
        changes the system; installer exit 1618 maps to a retryable busy error (A11) | WP6"
        and "One app instance only | WP6".

### [Issue 17] [`egm-design`] - Severity: Major — K10, the recorded version

*   **Affected Scope:** lines 36, 156, 389, 407
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid for the code on the branch. If ADR 0001 option A is accepted,
    the catalog version comes from winget and most of the risk goes away; the read after
    install is still the correct rule.
*   ***Proposed Edit:***
    *   Location: §6.7 gap table.
    *   Change: row "The recorded version is the catalog target, not the version that winget
        installed. After a success, read the version from the installed list; keep the target
        as the fallback and write a Warn row on a difference. | the F1 fix".

### [Issue 18] [`egm-design`] - Severity: Major — K11, `Failed` has more than one meaning

*   **Affected Scope:** lines 378-379, 393, 413, 435-437
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. The reviewer asks for a behavior change. §6.7 is a record of
    what is built, so the fix here records the defect and its owner; the behavior change is
    WP6 work with the reconcile (A10, Stage 5 finding I4).
*   ***Proposed Edit:***
    *   Location: §6.7, after the end table; gap table.
    *   Change: sentence: "`Failed` after a failed uninstall keeps the version: the app is
        probably still installed, and the ledger cannot say so." Gap row: "After a failed
        uninstall and after a restart, check the installed list and set `Installed` or
        `NotInstalled` from it; keep `Failed` for 'install tried, no version' (A10) | WP6".

### [Issue 19] [`egm-design`] - Severity: Major — K12, one error code for many outcomes

*   **Affected Scope:** lines 84-90, 367, 382-383, 391-395
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid. WP5 already makes the UI read `error.code` for consent, so the
    "busy" code belongs to WP5; the others wait (A24).
*   ***Proposed Edit:***
    *   Location: §1.3; §6.7 start table and gap table.
    *   Change: §1.3 lists each variant with its code. Gap rows: "'An operation is active'
        returns `unsupported`; add the code `busy`, retryable | WP5" and "Codes for a paid
        item, an illegal transition, timeout, rate limit and a busy installer (A24) | WP6".

### [Issue 20] [`egm-design`] - Severity: Major — K13, paid detection

*   **Affected Scope:** lines 205, 259, 266, 295, 314-318, 392
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** A17. No community package is paid, so the MVP is not affected; the
    rule must exist before WP7.
*   ***Proposed Edit:***
    *   Location: §4.2; §6.4.
    *   Change: `PackageSummary` gets `store_product_id` (optional). "A community package is
        never paid. An msstore item is paid, free or unknown; unknown gets the hand-off only.
        WP7 defines how the adapter reads the value."

### [Issue 21] [`egm-design`] - Severity: Major — K16, absent winget and minimum versions

*   **Affected Scope:** lines 71, 84, 396
*   ***AI Suggestion:*** ESCALATE
*   ***Justification:*** the finding is valid, and the values are the maintainer's to give.
    The skill says winget 1.6.0 or later. ADR 0001 option A needs 1.29 for the catalog, or a
    test on an older version. No file states a Windows minimum or what the UI shows when
    winget is absent (A8).
*   ***Proposed Edit:***
    *   Location: §2.
    *   Change: none until the maintainer gives: the minimum Windows version, the minimum
        winget version, and the screen for `WingetNotFound` (a message only, or a guide to
        install App Installer).

### [Issue 22] [`egm-comprehension`] - Severity: Major — X1 (Stage 6, C2), "stable state"

*   **Affected Scope:** lines 377-379, 411, 425-426, 498-499
*   **Discrepancy Description:** two definitions that disagree when the earlier state was
    `Failed`.
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** a comprehension gap is not contestable. The code is clear: a
    cancelled install with no version ends in `NotInstalled`.
*   ***Proposed Edit:***
    *   Location: §6.7 terms.
    *   Change: replace "stable state" with "rest state": "`Installed` when a version is
        recorded (`Pinned` when the pin flag is set); `NotInstalled` when no version is
        recorded." Use the term in the cancel and cleanup text.

### [Issue 23] [`egm-comprehension`] - Severity: Major — X2 (Stage 6, C5), return types

*   **Affected Scope:** lines 259-265
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** the types exist in the code; the fields are known from it.
*   ***Proposed Edit:***
    *   Location: §4.2.
    *   Change: add `OperationStarted` (operation id); `InstallStatus` (package id, state,
        installed version, target version, pinned, last updated, active operation with id and
        kind); `OperationLogEntry` (id, time, level, operation, package id, message, detail);
        `UpdatesSummaryEvent` (total, succeeded, failed, outcomes with package id, ok,
        message).

### [Issue 24] [`egm-zoom-out`] - Severity: Major — X3 (Stage 5, Z7), states that nothing writes

*   **Affected Scope:** lines 201, 212, 216-217, 389-394, 416
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** valid (A10). The code has the states, so the text marks them and
    does not remove them.
*   ***Proposed Edit:***
    *   Location: §4.1; §4.2; §4.3.
    *   Change: "`UpdateAvailable` and `Pinned` are reserved: nothing writes them before WP6."
        Remove `is_auto` and `auto_update` from the text, with a note that the column exists
        and is not used.

### [Issue 25] [all three] - Severity: Nit — N-A, §6.7 form and wording

*   **Affected Scope:** lines 343-345, 360-367, 379, 385-451
*   ***AI Suggestion:*** PROPOSE FIX
*   ***Justification:*** the lines are rewritten for Issues 1, 5 and 22 in any case.
*   ***Proposed Edit:***
    *   Location: §6.6, §6.7.
    *   Change: `####` headings in place of the bold labels. "The rows apply from top to
        bottom; the first match wins", with the "Any" rows moved to the top. A review id or
        "none" on each gap row; the fix text out of the gap column. Dates in place of "today".
        "After the app starts". State that one field of `AgreementsRequired` is true per
        error.

### [Issue 26] [`egm-style`, `egm-structural`] - Severity: Nit — N-B, remaining A25 items

*   **Affected Scope:** lines 25-28, 38, 60-66, 139, 147, 199-212, 215-222
*   ***AI Suggestion:*** DECLINE
*   ***Justification:*** recorded. These are wording items with no effect on what an
    implementer builds. Four of them are fixed as a side effect of Issues 3, 12 and 16
    ("Phase 3", "Eventually", "configurable", the duplicate lead-in). The others wait for the
    one A25 cleanup pass.

## Findings from Stages 5 and 6 that this report does not cover

- **Outside lines 1-454, for Stage 9:** Z8 (§8 against F2), Z10 (WP5 and WP7 scope), C7 (F1
  definition of done), C8 (guardrail 6 cannot pass), C9, C10, C18, C19.
- **Covered by an issue above:** Z1 (8, 9), Z2 (1), Z3 (4), Z4 (7), Z5 (10, 13), Z6 (3, 12),
  Z9 (11), I1 (2), I2 (13), I3 (4), I4 (18), I5 (16), I6 (already a WP5 gap row), C1 (9),
  C3 (5), C4 (10), C6 (13).
