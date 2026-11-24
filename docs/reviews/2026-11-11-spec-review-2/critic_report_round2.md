# Stage 7 — Critic Review, round 2 (re-audit)

- **Target:** `docs/SPEC.md` at commit `e51297c`, FeatureRange lines 1-592
- **Date:** 2026-11-12
- **Reviewers:** `egm-design`, `egm-structural`, `egm-style`. Each ran in a fresh context and
  did not get the round 1 findings.
- **Standard:** `feature-spec-standard.md` for design and structural. The style reviewer used
  its persona rules.
- **Raw totals:** 1 Blocker, 30 Major, 12 Nit. After the merge of duplicates: 1 Blocker,
  18 Major, 1 Nit group.
- **Re-audit cap:** this is the one permitted re-audit of Stage 7. No third round.

Finding ids: `design-2-n`, `structural-2-n`, `style-2-n` (reviewer, round, number).

## Verdict

Better, not ready. The three round 1 blockers did not come back: stored consent (K1), the
unchecked winget behavior (K4, now a spike) and the §6.6 against §6.7 contradiction (K5).
One new blocker and two design gaps remain in §6.6: the word "operation" has two meanings
(R1), the core trusts the webview that the user accepted (R2), and no flow covers an agreement
on search or detail if ADR 0001 option A is picked (R5).

## Compared with round 1

| Round 1 | Round 2 |
|---|---|
| K1 stored consent | Not raised again. |
| K2 absent sections | Partly open: no problem statement (R10); alternatives lack reasons (R4). |
| K4 unchecked winget behavior | Not raised again; the spike holds it. |
| K5 §6.6 against §6.7 | Not raised again. Two new edge cases in the tables (R7, R8). |
| C1 / Z1 contract or plan | Open in a new form: "built" does not say which branch (R3). |

## Blocker

### R1 — "Operation" has two meanings in §6.6

- **Raw:** style-2-1. **Lines:** 420, 460-466, 471-473.
- "Operation" means one winget run (one `operation_id`) and also the full user attempt across
  consent retries. The run ends at step 2, so a retry is a new operation. The text then says
  "one operation only" and "the dialogs of this operation". An implementer cannot tell if the
  UI must carry the earlier accepted kinds into the retry.
- **Recommendation:** define "attempt" as the chain of starts that consent retries link. Say
  that the UI sends the kinds accepted in the attempt to each new operation.

## Major

### R2 — The core cannot check that the user accepted

- **Raw:** design-2-1. **Lines:** 182, 420-421, 460-464, 481-482.
- The core passes an agreement flag when the IPC caller sends `options.consent`. The webview
  also shows remote catalog text, and the content policy covers images only. Injected script
  can send a forged consent. That breaks hard rule 4.
- **Recommendation:** add a trust-boundary rule. Either the core opens the consent dialog, or
  the spec requires a strict CSP and sanitised catalog text. Add the risk to §2.3.

### R3 — "Built" does not say which code

- **Raw:** structural-2-1, style-2-3, design-2-9. **Lines:** 8-14, 177, 190, 373, 397, 412,
  494, 576-579.
- WP5 is on an unmerged branch (PR #19). §6.2, §6.4 and §6.7 carry "WP5, built", and §2.3 says
  the spike "comes first in WP5". The rule "the code is the fact" has no fixed reference.
- **Recommendation:** add a label *built on branch*, or name the branch that counts. Say that
  the spike and §6.6 are the WP5 work that remains and that they block the merge of PR #19.

### R4 — Alternatives have no reasons; §6.6 has no alternatives

- **Raw:** structural-2-2, design-2-2. **Lines:** 162-167, 412-492.
- In §2.2, catalog options (d) and (e) have no reason, the UI shell row only links to
  `docs/STRATEGY.md`, and two rows are "Open". §6.6 shows one design. It does not discuss a
  visible winget prompt or the COM API.
- **Recommendation:** give each rejected option one line of trade-off. Add an alternatives
  table to §6.6.

### R5 — No agreement flow for search and detail

- **Raw:** design-2-3. **Lines:** 165, 364-374, 470-473.
- ADR 0001 option A moves search and detail to winget CLI calls. The ADR says nobody checked
  if the `winget` source asks for an agreement on a new machine. If it does, search,
  `get_app_detail` and the detail read in `start_install` can end in `agreements_required`.
- **Recommendation:** name search and detail in the read-only consent paragraph, or make §6.6
  depend on the ADR result.

### R6 — The slow first `winget show` is not a listed risk

- **Raw:** design-2-4. **Lines:** 178-183, 374.
- Under option A the first `winget show` takes 21-23 s. `start_install` reads the detail
  first, so the first Install click can wait that long.
- **Recommendation:** add the risk to §2.3. State that `start_install` uses the cached detail
  or only the `is_paid` value, with a time limit.

### R7 — A cancelled uninstall from `Failed` can end in `Installed` with no version

- **Raw:** design-2-5. **Lines:** 500-501, 519, 538.
- `start_uninstall` is legal from `Failed`, also after a failed first install. The cancel row
  then sets `Installed`. That breaks the rest-state rule. The reviewer reports that `settle`
  in `services/install.rs` does the same; the main context did not check this.
- **Recommendation:** refuse `start_uninstall` from `Failed` with no version, or send the
  cancel to the rest state.

### R8 — The agreement row can need an illegal edge

- **Raw:** design-2-6. **Lines:** 504, 539, 587.
- The planned row restores "the state before the operation started". `Installing ->
  UpdateAvailable` and `Uninstalling -> UpdateAvailable` are not legal edges. After WP6 the
  restore fails and the row stays `Installing`.
- **Recommendation:** make the target the rest state, or add the edges in the same WP5 change.

### R9 — Hard rule 5 conflicts with remote images and the hand-off

- **Raw:** design-2-7, style-2-11. **Lines:** 55-56, 182, 400.
- Rule 5 permits network calls only to the enabled catalogs and winget. The app loads icons
  and screenshots from any HTTPS host, and the image proxy decision waits for WP8.
- **Recommendation:** limit image hosts with a CSP `img-src` list now, or reword rule 5 and
  list the permitted destinations.

### R10 — No problem statement

- **Raw:** structural-2-3. **Lines:** 3, 112-122.
- The user problem and the users are only a link. The feature-spec standard requires them in
  the document.
- **Recommendation:** add a short problem statement before §1.

### R11 — `is_paid` cannot hold "unknown"

- **Raw:** structural-2-4. **Lines:** 44-46, 274, 402-404.
- §6.4 has three cases: paid, free, unknown. `is_paid` is a boolean.
- **Recommendation:** make the field an optional boolean, or say how "unknown" maps to it.

### R12 — Terms are used before their definition

- **Raw:** structural-2-5. **Lines:** 181, 334-337, 374, 498.
- "Ledger", "active operation" and "rest state" appear from §2.3. Their definitions are in
  §6.7. §0 has the title Terminology and does not hold them.
- **Recommendation:** move the §6.7 terms to §0.

### R13 — Types with no definition

- **Raw:** structural-2-6, style-2-17. **Lines:** 278, 283-284, 311, 334, 460-461, 481.
- No definition exists for the operation kind enum, the `request` argument of `install`, the
  values of `scope`, and the shape of `options.consent`. The field `operation` means the kind,
  next to fields that mean the id.
- **Recommendation:** add `OperationKind` to §4.1 and a request struct with `consent` to §4.2.
  Rename the field to `operation_kind`.

### R14 — §5.4 does not show the planned consent arguments

- **Raw:** structural-2-7, style-2-15. **Lines:** 339, 345, 394-395, 470-473, 481-482.
- §6.6 adds `options.consent` to `check_updates` and `start_uninstall`. §5.4 lists
  `check_updates` with no arguments. `update_all` has a rule for a package agreement and none
  for a source agreement. The "library list" of line 470 is not in the WP5 change list.
- **Recommendation:** add the planned arguments to §5.4 with a *planned (§6.6)* mark. State
  the `update_all` behavior for the two agreement kinds.

### R15 — "Source" has two meanings

- **Raw:** style-2-2. **Lines:** 63, 293, 320, 366, 389, 429, 578.
- "Source" is an app `CatalogSource` adapter and also a winget source (`winget`, `msstore`).
  "Source agreement" can mean one or the other.
- **Recommendation:** write "catalog source" and "winget source" in each place.

### R16 — Pinning and `UpdateAvailable` each have two forms

- **Raw:** style-2-4, style-2-5, structural-2-10. **Lines:** 266-267, 280, 282, 500-501, 541,
  582.
- Pinning is a state (`Pinned`) and a flag (`installs.pinned`). Line 541 has built code that
  returns to `Pinned`; line 267 says nothing writes it before WP6. `UpdateAvailable` is a
  state variant and a struct.
- **Recommendation:** pick one form for pinning and make lines 267 and 541 agree. Rename the
  struct.

### R17 — "Version not available" uses the wrong error

- **Raw:** style-2-14. **Lines:** 109, 463-464.
- The case returns `agreements_required`. That error is defined as "exactly one agreement
  field is true", and no field fits.
- **Recommendation:** give the case its own code, or state the field.

### R18 — Three names for the actor that settles the ledger

- **Raw:** style-2-8. **Lines:** 356, 527, 560, 587.
- The text names the executor, the core and the service layer. "Warn log row" does not say
  `operation_log` or tracing.
- **Recommendation:** name `services/install.rs` and write `operation_log` in full.

### R19 — Seven small ambiguities

- **Raw:** style-2-6, -7, -9, -10, -12, -13, -16.
- "Stale" means expired cache and also wrong F1 data (365-366). Two rules select the executor
  (314, 322). Hard rule 1 offers two hand-off URLs (46-47 against 33, 341).
  "Destructive-or-silent" has no definition, and each install is silent (52, 376). "v1" has no
  work-package range (157, 314, 389). The id example `src/pkg` does not say if `package_id`
  includes the source (272, 289). "First use of the core" is not a named event (565).
- **Recommendation:** one sentence of definition for each.

## Nit group

### R20

- **Raw:** structural-2-8, -9, -11, -12, -13; design-2-8, -10, -11, -12; style-2-18.
- §6.3 carries WP6 and the gap table gives its fix to WP5 (389-391, 578). Five UI components
  have no WP mark (249-250). Goals come after the error model (112-129). `OperationStage`
  values are lowercase (264-269). A-ids have no meaning in this file (156 and others).
  `is_auto` is absent from §4.2, and code already uses `auto_update` (280, 291-292). The
  sources named on read-only calls after WP7 are not stated (389-391). The winning source of
  a merged search card is not stated (320-322). §6.6 depends on the CLI exit codes if the COM
  executor is picked (166). Wording items: see style-2-18 in the reviewer output.

## Notes from the main context

- R1, R2, R8, R14 and R17 are defects in the §6.6 and §6.7 text that this session wrote in
  commit `7b0e6e0`.
- R5 and R6 depend on the pick in ADR 0001. They close faster after that pick.
- R7 is a code claim. Check `services/install.rs` before any edit to the table.
- R11 and R16 can change the data model. They need a decision, not only wording.
