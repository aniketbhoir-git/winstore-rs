# Stage 7 — Critic Review, round 1

- **Target:** `docs/SPEC.md` at commit `961b63d`, FeatureRange lines 1-454
- **Date:** 2026-11-11
- **Reviewers:** `egm-design` (completed), `egm-structural` (completed), `egm-style`
  (completed). Each ran in a fresh context.
- **Standard:** `feature-spec-standard.md` for design and structural. The style reviewer used
  its persona rules.
- **Raw totals:** 4 Blocker, 29 Major, 35 Nit. After the merge of duplicates: 3 Blocker,
  21 Major, 2 Nit groups.

Finding ids: `design-1-n`, `structural-1-n`, `style-1-n` (reviewer, round, number). The
reviewer's own label is in brackets.

## Verdict

Not ready. Do not start code on §6.6. The consent design can still accept changed terms with
no user action (K1), it contradicts §6.7 (K5), and it rests on a winget behavior that nobody
checked (K4).

## Blockers

### K1 — Stored source consent can accept changed terms silently

- **Raw:** design-1-2 [B2]. **Lines:** 52-53, 334-342, 348-350, 363.
- The `sources` table stores only the time of consent. Each later operation on that source
  then passes the source agreement flag. The reviewer believes, and did not verify, that winget
  asks again when the terms of a source change. If so, the stored consent stops each later
  prompt. That is the silent acceptance of hard rule 4, moved from a config flag to a database
  column. The revoke path is deferred, so the user cannot stop it.
- **Recommendation:** pass the source flag only on the retry directly after the user accepts.
  Let winget keep the acceptance. If the app must keep a record, store the agreement identifier
  or a hash of the shown text, and treat a mismatch as no consent.

### K2 — Required sections are absent: problem, goals and non-goals, alternatives, risks

- **Raw:** design-1-1 [B1], structural-1-1 [B1], structural-1-8 [M1], structural-1-9 [M2].
  **Lines:** whole range; rationale only at 111-114.
- The standard fails a spec that shows one path and no rejected option. §2 is a table of
  locked decisions. No section states the problem, the goals or the non-goals. Risks appear
  only as the "Known gaps" table, which lists absent work.
- The first review (A22) lowered this to Major and limited it to three decisions. The standard
  as written makes it a hard fail. The main context must choose.
- **Recommendation:** apply A22 as the minimum. A two-line objective; goals and non-goals with
  links to STRATEGY §1.2, §1.3 and §5; an alternatives entry for the catalog backend, CLI or
  COM executor, and the headless shape; a risks section with only the risks that this spec
  adds.

### K3 — Headless mode still describes a localhost install API with no authentication

- **Raw:** design-1-3 [B3], style-1-9 [M9], structural-1-16 [N8]. **Lines:** 61, 69-70, 108,
  267, 320-325. Blocks WP10 only. Review action A5 is open.
- Any web page in a local browser can reach a localhost API. axum is still in the "Locked"
  table. The `accept_package_agreements` option of §6.6 would be reachable over HTTP.
- **Recommendation:** cut §6.5 to one sentence that gates WP10 on a decision record. Move axum
  to a deferred list.

## Major — agreement consent (§6.6)

### K4 — The dialog depends on a winget behavior that nobody checked

- **Raw:** design-1-4 [M1]. **Lines:** 346-347, 371-373.
- The dialog must show the agreement text. Operations run with no interactive prompt, and the
  skill marks the agreement prompt lines as INFERRED. The only stated outcome of a failure is
  "stop and report".
- **Recommendation:** run a spike with a pass or fail result before the WP5 work. Define the
  fallback now: a dialog with the license URL from the manifest. Record in §6.6 that the
  fallback satisfies rule 4.

### K5 — §6.6 and §6.7 contradict each other on the ledger after `agreements_required`

- **Raw:** design-1-5 [M2]. **Lines:** 343-345, 353, 378-379, 408-409, 413.
- Step 2 sends the ledger through the "failed" rows. Those rows give `Failed` and an Error log
  row for a first install. Step 5 then says a decline does not change the ledger.
- **Recommendation:** add a row to the end table: outcome `agreements_required`, ledger as
  before the operation, Info log row, code `agreements_required`. Point step 2 at that row.

### K6 — Package consent is a boolean that any caller can set

- **Raw:** design-1-6 [M3]. **Lines:** 337, 348-350, 364.
- The core cannot tell that the user saw a dialog. The retry is also not tied to the version or
  the text that the user approved.
- **Recommendation:** `AgreementsRequired` returns a one-time consent token tied to the package
  id, version, source and a hash of the shown text. `start_install` takes the token and refuses
  a mismatch.

### K7 — Read-only and batch paths have no agreement rule

- **Raw:** design-1-7 [M4]. **Lines:** 264, 307-309, 341-342, 365-366.
- `check_updates` and `list_installed` can fail on a source agreement. The spec does not say
  what they return then. The `update_all` sentence "stops at a package ... and continues with
  the next" contradicts itself.
- **Recommendation:** define the `check_updates` result for that case. Check source consent one
  time before a batch. Reword the batch rule to "skips a package that needs a package agreement
  and records it in the summary".

### K8 — The spec sends the reader to a skill that says the opposite

- **Raw:** design-1-8 [M5], structural-1-11 [M4]. **Lines:** 17-18, 297-299, 358-362; skill
  lines 19 and 61.
- **Recommendation:** add a note at the pointer that §6.6 replaces the agreement rows of the
  skill. Add "update the `winget-executor` skill" to the list "Changes that WP5 owns".

## Major — state, locks and data

### K9 — The lock is per package and in one process

- **Raw:** design-1-9 [M6]. **Lines:** 79-80, 311, 395, 433-438.
- Two installs of different packages can run together; Windows Installer permits one MSI
  transaction per machine (error 1618). Nothing requires one app instance. Restart recovery
  does not look for a winget process that survived.
- **Recommendation:** require a single instance or a machine-level lock. State the single queue
  as the target and assign it to WP6. Map exit 1618 to a retryable "busy" error. Remove
  "configurable".

### K10 — The recorded version comes from a stale catalog

- **Raw:** design-1-10 [M7]. **Lines:** 36, 156, 389, 407.
- On success the ledger sets the version to the target. With no user choice, the target is the
  latest version of a catalog that stopped in 2023.
- **Recommendation:** after exit 0, read the version with `winget list --id ID --exact`. Use
  the target as the fallback and log a Warn row on a difference.

### K11 — `Failed` has more than one meaning

- **Raw:** design-1-17 [M14]. **Lines:** 378-379, 393, 413, 435-437.
- A failed uninstall sets `Failed` and keeps the version; the app is most probably still
  installed. The stable-state text says `Failed` means no version.
- **Recommendation:** after a failed uninstall, go back to `Installed` and keep the error in
  the log. Keep `Failed` for "install tried, no version".

### K12 — One error code covers outcomes that the UI must separate

- **Raw:** design-1-14 [M11]. **Lines:** 84-90, 367, 382-383, 391-395.
- An illegal transition, a paid-item refusal and "an operation is active" all return
  `unsupported`. §6.6 tells the UI to branch on the code.
- **Recommendation:** add `busy` (retryable), `paid_item` and `illegal_transition`. Add
  `Timeout`, `RateLimited` and `InstallerBusy` (A24).

### K13 — Paid detection fails open, and the product id has no field

- **Raw:** design-1-13 [M10]. **Lines:** 205, 259, 266, 295, 314-318, 392.
- **Recommendation:** make `is_paid` three-valued, or make each source state its rule. Treat
  unknown as hand-off only for msstore items. Add `store_product_id` to the §4.2 structs (A17).

## Major — catalog and sources

### K14 — `winget.run` is a sanctioned interface and the cause of the MVP bug

- **Raw:** design-1-11 [M8]. **Lines:** 36, 156, 288-292.
- **Recommendation:** mark the catalog backend in §0 and §3 "pending the decision record (F1,
  #16)". Add the risk "the third-party catalog is stale or wrong" with its mitigation. Cache
  detail only (A15).

### K15 — The msstore source is still described as the Store REST service

- **Raw:** design-1-12 [M9], style-1-33 [N12]. **Lines:** 32, 48-49, 157.
- **Recommendation:** "winget `msstore` source, reached only through the winget CLI"; off by
  default; `SourceUnavailable` on failure (A6).

### K16 — No rule for an absent winget, and no minimum versions

- **Raw:** design-1-16 [M13]. **Lines:** 71, 84, 396.
- **Recommendation:** state the Windows and winget minimums in §2. Define the screen for
  `WingetNotFound` (A8).

### K17 — The no-telemetry rule is not in §0

- **Raw:** design-1-15 [M12]. **Lines:** 42-53, 206.
- The backlog tells contributors to give a model only §0. Remote icons and screenshots send
  the user's IP address to other hosts.
- **Recommendation:** add the rule as hard rule 5. Enforce it with a content security policy
  that limits image hosts (A23).

## Major — the document contradicts itself

### K18 — The status framing

- **Raw:** structural-1-10 [M3], style-1-8 [M8], design-1-18 [N1]. **Lines:** 13-16, 249, 320,
  327, 375, 288-325.
- The note says WP0 to WP5 is the contract for code that exists. §6.6 is WP5 and not built.
  WP5 is on a branch, not on `main`. §6.2 step 2 states agreement behavior that is not built.
  Status markers have three forms.
- **Recommendation:** "WP0 to WP4 are on `main`. WP5 is on branch `impl/6--install-loop`
  (PR #19), except §6.6 and the WP5 rows of the gap table, which are plan." Give each §6
  subsection one marker in one format. Mark §6.2 step 2 as target behavior.

### K19 — §6.1 against the §5.4 command table

- **Raw:** structural-1-12 [M5], style-1-7 [M7], design-1-19 [N2]. **Lines:** 257, 288-292.
- **Recommendation:** rewrite §6.1 with `search_sources(search_id, query)` and the events.
  Remove `filters` or name the work package that adds it.

### K20 — The §3 tree against the list above it

- **Raw:** structural-1-13 [M6], style-1-6 [M6]. **Lines:** 120-192.
- `services/search.rs` is "not planned" and is in the tree. `ipc/tests.rs` is in neither
  place. The tree puts `CatalogSource` in `domain/source.rs`; the code has it in `catalog/`.
- **Recommendation:** remove `services/search.rs`. Add the built files to the tree. Correct the
  `CatalogSource` comment.

### K21 — Framing that addresses the wrong reader, and claims that are not measured

- **Raw:** style-1-1 [M1], style-1-2 [M2], style-1-3 [M3], style-1-4 [M4], style-1-5 [M5],
  design-1-21 [N4], design-1-22 [N5], structural-1-15 [N2]. **Lines:** 1, 3-11, 23, 50, 94-114,
  196, 204, 227, 286.
- The purpose block defends the document. Headings give orders to a code generator ("LLM must
  implement exactly these") for code that exists. Rule 3 forbids the label that §6.4 requires.
  The "Locked" table holds "Svelte (or React if preferred)", Scoop, Chocolatey and axum. "Far
  safer" and "fast" have no evidence.
- **Recommendation:** one sentence of purpose. Remove the bracket instructions from headings.
  Rule 3 permits the text label "Buy in Microsoft Store" and forbids logos and artwork. Lock
  Svelte; move the three others to a deferred list. Replace the rationale with plain facts
  (A25).

## Nits

**N-A — §6.7 form and wording** (structural-1-14 [N1], structural-1-22 [N9], style-1-37 to
style-1-42 [N16-N21], design-1-20 [N3]):
- Bold labels in §6.7 work as headings and cannot be linked. Use `####`.
- The start table has "Any" rows that overlap the state rows. State the order: top to bottom,
  first match wins.
- Give each gap row a review id or "none". Move the fix out of the gap text at line 444.
- "Not a failure in the UI": say what the UI does. "Today": give the date. "After a start":
  say "after the app starts". "`NotInstalled` or `Failed` when not": state the rule.
- `AgreementsRequired` has two fields, and winget reports one kind at a time. State that one
  field is true per error, or use one `kind` field.

**N-B — remaining A25 items** (style-1-22 to style-1-36 [N1-N15], structural-1-17 to
structural-1-21 [N3-N7]):
- Duplicate lead-in at lines 25-28. The paid rule appears three times (lines 38, 44, 314).
- "Silent" has two meanings in rule 4 and §6.2.
- "Eventually", "Phase 3", "pluggable", "styled to look like a modern app store", "prevents
  races", "best-effort", "stable" id, "configurable concurrency = 1".
- Name `AppError` in §1.3 and list the code of each variant.
- Two `ProgressEvent` field lists. Mixed enum casing. `UpdateAvailable` is a state and a
  struct.
- Missing space at `package_manager.rs#`. "+" in place of "and". Templated URLs need code
  spans; file references need relative links.

## Reviewer status

| Reviewer | Status | Blocker | Major | Nit |
|---|---|---|---|---|
| `egm-design` | completed | 3 | 14 | 5 |
| `egm-structural` | completed | 1 | 6 | 9 |
| `egm-style` | completed | 0 | 9 | 21 |

## Notes from the main context

- **K1, K4, K5, K6, K7, K8 and K18 are in text that I wrote in this session.** Five reviewers
  in three stages now report the same core defect in §6.6: it fixes storage, a command and a
  retry loop before the facts about winget are known.
- **K1 and K4 rest on facts about winget that no reviewer verified:** how winget stores source
  agreements, if it asks again after a change, and if it gives agreement text with no prompt.
  A spike on a real machine answers the three.
- **K2:** `docs/adr/0001-community-catalog-backend.md` now holds the alternatives for the
  catalog backend. It was written after commit `961b63d`, so the reviewers did not see it.
- **K10:** if ADR 0001 option A is accepted, the catalog version comes from winget itself, and
  most of K10 goes away. The read of the real version after install is still correct.
- **K11:** the code today gives `Failed` after a failed uninstall. This finding asks for a code
  change, not only a text change.
