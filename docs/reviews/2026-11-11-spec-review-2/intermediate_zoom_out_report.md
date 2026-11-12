# Stage 8 — Intermediate Zoom Out Review

- **Target:** `docs/SPEC.md` at commit `e3e151c`, full file (786 lines). Feature part: lines
  1-694. Work packages: from line 695.
- **Date:** 2026-11-12
- **Reviewer:** `egm-zoom-out`, fresh context, zoom-out standard, with the Stage 5 report.
- **Totals:** 0 Blocker, 3 Major, 4 Nit. Stage 5 Blockers and Majors: 9 closed, 1 mostly
  closed, 3 open.

## Verdict

The fixes closed the Stage 5 Blocker, and §6.6 is close to the smallest design that satisfies
hard rule 4. But the fixes put read-only consent and bug F1 into WP5, and the work-package
part (from line 695) still contradicts the feature part. Make WP5 smaller and rewrite §7 and
§8 before code starts.

## Status of the Stage 5 findings

| Stage 5 finding | Status |
|---|---|
| Z1 (Blocker): status note, §6.6 and the skill contradict | Closed. |
| Z2: stored source consent | Closed. |
| Z3: §6.6 rests on an unknown | Closed. The spike comes first; a fallback is named. |
| Z4: read-only commands can need an agreement | Closed. The fix added scope: see IZ-1. |
| Z5: the catalog design assumes a backend that F1 can replace | Open in a new form. `get_featured`, `list_categories` and the merge, dedupe and paging step stay; a planned WP7 merge rule was added. |
| Z6: §2 locks choices that are still gated | Mostly closed. Open in WP10 and §8: see IZ-3. |
| Z7: states that nothing writes | Closed. Marked reserved. |
| Z8: §8 contradicts F2 and the skill | Open. §8 is not changed: a real install in CI, a Server runner, a manual checklist. |
| Z9: §3 keeps three layouts | Closed. |
| I1: the CLI is the only integration surface | Closed. An open decision before WP6. |
| I2: search depends on a third-party mirror | Closed. §0 points to ADR 0001. |
| I3: the agreement flow fails first, then asks | Open. The alternatives table does not record a check before the operation, or why it was rejected. |
| I4: the ledger is an authority, not a cache | Closed. Deferred to WP6. |

## New findings

### Major

**IZ-1 — lines 397, 565-571, 579-580 — read-only consent is in WP5 with no user.** WP5 adds
a `consent` option to `check_updates` and a general rule for read-only consent.
`check_updates` has no UI before WP6. It names only the `winget` source, and the ADR research
saw no prompt from that source. The rule also reaches to `search_sources` and
`get_app_detail` for an ADR result that is not decided.
*Recommendation:* remove read-only consent from WP5. State that a read-only
`agreements_required` shows as an error. Move the rule to WP6, or to WP7 when msstore is on.

**IZ-2 — lines 8-10, 677-681, 707, 734-739 — WP5 is too large, and PR #19 waits for too
much.** The close condition of WP5 now holds all of §6.6 (spike, CSP, dialog, consent on
three commands, skill update, tests), five gap rows with a new `busy` error code, and all of
F1, which is a catalog backend change. The status note ties the merge of PR #19 to this
work. "Merge" (line 10) and "close" (line 707) do not say the same thing.
*Recommendation:* gate the merge of PR #19 on §6.6 and the cancel and rest-state rows only.
Track F1 as #20 and let it gate the MVP milestone, not the PR. Move `busy` to WP6 with the
other A24 codes.

**IZ-3 — lines 718, 734-735, 742, 744-745, 755, 758-759 — the work packages contradict the
feature part.**

- WP2 says "public JSON catalog endpoint"; ADR 0001 proposes option (b).
- WP5 lists screenshots and rating, which the community source cannot give (Z10, open).
- WP6 requires "pins hold"; no pin command exists and the pin model changes in WP6.
- WP7 lists the Buy-in-Store button, which is built on the branch.
- WP10 specifies "axum API + feature flag `server`" against §2.1 and §6.5.
- WP11 includes offline MSIX, which §2.1 defers to A9.

*Recommendation:* rewrite the WP2 and WP5 to WP11 entries. Each one points to §2.1, §6.5 or
the ADR and does not repeat a design that was rejected. Remove items that are built.

### Nit

- **IZ-4 — lines 526-535.** The spike says "Three facts" and lists four. Questions 2 and 3
  cannot change the design. Keep questions 1 and 4 as the gate; mark 2 and 3 as notes for the
  skill.
- **IZ-5 — lines 62-65, 553-561.** The attempt term and the shown-version rule are the only
  parts of §6.6 above the minimum. Keep them, and add one sentence of reason for the version
  rule: it stops a change of terms between the dialog and the install.
- **IZ-6 — lines 103, 264-265, 270-272, 286, 360.** §1 and the §3 tree still list WP9, WP10
  and WP11 paths that §2.1 defers. Remove them from the tree.
- **IZ-7 — lines 8, 697.** The header has the date 2026-11-12 and §7 has 2026-11-11. Use one
  date.

## Notes from the main context

- **IZ-1 and IZ-2 are scope that the round 2 fixes added.** R5 and R14 made the read-only
  rule wider; the reviewer is correct that WP5 does not need it.
- **IZ-2 needs a maintainer decision:** what gates the merge of PR #19. `CLAUDE.md` says
  "Do not merge it before review actions A2 and A4 are closed". A4 is closed. A2 is §6.6.
- **IZ-3, Z8 and Z10 are in the Stage 9 range** (line 695 to the end). Stage 9 will find them
  again with more detail. A rewrite of §7 and §8 before Stage 9 saves that round.
- **I3:** the check before the operation is not possible until the spike answers question 1.
  The alternatives table must say so.
