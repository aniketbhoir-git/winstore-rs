# Stage 5 — Initial Zoom Out Review

- **Target:** `docs/SPEC.md` at commit `961b63d` (full file)
- **Date:** 2026-11-11
- **Reviewers:** `egm-zoom-out`, `egm-inventive-catalyst`, each in a fresh context
- **Standard:** `zoom-out-standard.md`
- **Context given:** `docs/STRATEGY.md`, `docs/ISSUE-BACKLOG.md`, `CLAUDE.md`,
  `docs/reviews/2026-11-11-spec-review.md`, `.claude/skills/winget-executor/SKILL.md`
- **Totals:** 1 Blocker, 12 Major, 7 Nit

Both reviewers could read their persona and the standard. Neither searched the code.

## Verdict

Do not start code on §6.6 (agreement consent). One Blocker and four Major findings are on
§6.6. The repair for A1, A3 and A4 holds. Review actions A5, A9, A10, A11, A15, A20 and A25
from the first review are still open in the text.

## Zoom-out reviewer

### Z1 — Blocker — three sources contradict each other on §6.6 (lines 13-19, 297-299, 358-362)

- The status note says that for WP0 to WP5 "the code is the fact". The code has the global
  `accept_source_agreements` flag.
- §6.6 says: remove that flag.
- The spec sends the implementer to the `winget-executor` skill for flags. `SKILL.md` line 19
  and line 61 say to pass the two accept flags "ONLY when `AppConfig.accept_source_agreements`
  is true". Its test list (line 153) asserts the same.

A model with no memory that follows the spec's own pointer will rebuild the silent acceptance
that hard rule 4 forbids.

**Recommendation:** in the status note, exclude the parts marked "not built": for those, the
text is the plan and overrides the code. Add the `SKILL.md` update (hard-rule row, the flag
note, the test list) to the §6.6 list "Changes that WP5 owns".

### Z2 — Major — source consent storage copies state that winget can keep itself (lines 332-337, 341-342, 355-356, 363-364)

The design adds a column, a command and a revoke path. The reviewer believes, and did not
verify, that winget records source-agreement acceptance itself, and that source agreements are
mainly a matter of the msstore source, which is off until WP7.

**Recommendation:** check the two facts in the §6.6 spike, before the migration. If they hold,
WP5 builds only the package-agreement path. Defer the column, the command and the revoke path
to WP7. Step 6 (revoke in settings) is not necessary now in any case.

### Z3 — Major — §6.6 rests on its largest unknown (lines 346-347, 371-373)

The dialog must show the agreement text. Nobody knows if winget gives that text with no
prompt. The skill marks the agreement prompts as INFERRED. The design already fixes a column, a
command, an option and a retry loop.

**Recommendation:** order the work as spike, then skill record, then design. Name the fallback
now, for example the license and agreement fields from `winget show`, or the license URL.

### Z4 — Major — read-only commands can also need an agreement (lines 240-242, 307-309, 339-344)

`check_updates` runs `winget upgrade` with no `--source`, so it queries each configured
source, msstore also. A source with terms that the user did not accept can make a read-only
call fail. §6.6 covers the start commands only.

**Recommendation:** in v1, limit the read paths to `--source winget`. State it in §6.3 and
§6.6.

### Z5 — Major — the catalog design assumes a backend that F1 can replace (lines 36, 111-114, 229-233, 244-247, 288-292, 543-544)

§2 justifies the separate HTTP catalog layer by speed and typed search. §5.3 merges, dedupes
and pages across sources. There is one source, and it returns unrelated results. One F1 option
is `winget search` through the executor; if it wins, §2, §5.1, §5.3 and §6.1 change.
`CatalogSource` also requires `get_featured` and `list_categories`, which only WP8 needs.

**Recommendation:** mark those sections "pending the F1 decision record". Reduce §5.3 to
single-source search and defer dedupe and paging to WP9 (A15). Move `get_featured` and
`list_categories` to WP8. Declare the endpoint in use in one place (A14).

### Z6 — Major — §2 locks choices that are still gated (lines 61, 104-108, 235-239, 267, 320-325)

axum headless mode waits for a decision record (A5). The `PackageManager` executor conflicts
with "one executor in v1" (A9). Scoop and Chocolatey are far away. §6.5 still gives headless
detail. §5.2 describes a registry for executors that do not exist.

**Recommendation:** move axum, `PackageManager`, Scoop and Chocolatey to a "deferred, needs
decision" list. Replace §6.5 with one line: "WP10; blocked on a decision record (A5)". Change
the §5.2 rule to "chosen by `SourceKind`; v1 has one executor".

### Z7 — Major — the state model carries states that nothing writes (lines 201, 212, 216-217, 389-394, 416, 448)

`UpdateAvailable` and `Pinned` are in `InstallState`; the gap table says nothing writes them.
`auto_update` and `is_auto` have no work package. §6.7 carries table rows and a rule for the
two states.

**Recommendation:** do not remove them in WP5, because the code has them. Mark them "reserved,
not written until WP6" in §4.1. Put the move of pin to a flag into WP6. Drop `auto_update` and
`is_auto` from §4.

### Z8 — Major — §8 contradicts F2 and the skill (lines 532-535)

§8 names a CI test that installs and removes a real package, and a manual checklist. The skill
says "CI never runs real winget". F2 says no test needs a person.

**Recommendation:** reduce §8 to what runs today. Add one line that points to the F2 design.
Remove the Windows Server runner line until WP10.

### Z9 — Major — §3 keeps three layouts that disagree (lines 120-192)

A target tree, a list of differences, and a pointer to `CLAUDE.md`. The tree still shows
`services/search.rs`, which the list says is not planned. The list will go out of date with
each pull request.

**Recommendation:** delete the paths that are not planned. Replace the tree with a table of
files per work package for WP6 to WP11 (A21). Point to `CLAUDE.md` for the layout as built.
Drop the difference list.

### Z10 — Nit — WP5 and WP7 scope overlaps what is built (lines 495, 504-505)

WP5 lists screenshots and rating, which the community source cannot give (A16). WP7 lists the
Buy-in-Store button, which is built (A19).

### Z11 — Nit — the §6.6 change list holds WP6 behavior (lines 365-366)

The `update_all` agreement behavior belongs to WP6.

### Z12 — Nit — "configurable concurrency = 1" is still in the text (line 311)

A11 asked for its removal. Say "sequential".

### Z13 — Nit — open wording items from A25 and A23 (lines 3-11, 23-28, 50-51, 320)

"Safe to paste" and the named model; the duplicate lead-in; "(give these to the coding LLM)" in
headings; rule 3 does not permit the text label "Buy in Microsoft Store"; "Phase 3" in place of
WP10; the no-telemetry rule is not in §0.

## Inventive-catalyst reviewer

This persona gives advisory findings, Major or Nit only.

### I1 — Major — the winget CLI is the only integration surface (lines 111-114, 235-242, 297-300, 419-431, 482-487)

The spec rejects text parsing for search and accepts it for progress, update lists and
installed lists. It accepts cancel by a stop of the process tree. Both follow from the CLI
only. The documented COM API `Microsoft.Management.Deployment` has typed search, an install
call with typed progress and cancel, and typed agreement objects.

**Proposal:** a decision record "CLI or COM executor" (A22 lists it). Keep `winget_cli.rs`. Add
a COM executor behind the same traits for WP6 or WP11, with the CLI as the fallback.

**Costs:** the `windows` crate has no bindings; access from an app with no package identity
must be checked first; a second test fake; two code paths.

### I2 — Major — search depends on a third-party mirror (lines 36, 111-114, 156, 288-292)

The mirror is the root cause of F1, and the cache makes the bad rows last longer. Mirror ids
may not match winget ids, which forces the id check in A14.

**Proposal:** read the official winget source index locally with SQLite, or use the COM
`FindPackages` call. Remove `winget.run` and `winget-api.dev` from §0.

**Costs:** the index schema is not a documented contract and can change.

### I3 — Major — the agreement flow fails first, then asks (lines 339-373)

The flow runs an operation until winget fails. The ledger moves to `Installing` and back. The
dialog can open two times. `update_all` stops at each package. The flow also rests on the open
point about the agreement text. The skill still tells the implementer to pass the two flags
from the config value.

**Proposal:** check before the start with a read-only call (`winget show`, or the COM agreement
objects). Show one dialog for the two kinds. For `update_all`, check each package first and
collect all consents. Keep fail-then-retry as the fallback. Fix the skill row in the same pull
request.

**Costs:** one more call before each install; a parse of `winget show` text; the install must
pin the version that the check saw; two consent paths to test.

### I4 — Major — the ledger is an authority, not a cache (lines 216-217, 433-438, 448-450)

On restart the ledger guesses: `Uninstalling` becomes `Failed`. Apps installed outside the app
have no place. Several gaps come from the same assumption.

**Proposal:** define the ledger as a cache of `list_installed()` plus the app's own running
operations. In WP5, after a restart and after a failed or uncancellable operation, run a
read-only `winget list --id ID --exact` for each affected row. In WP6, add the full reconcile.

**Costs:** `winget list` takes seconds; a failed parse must leave the state unchanged; the
check must take the same per-package lock.

### I5 — Nit — per-package lock against the machine-wide installer lock (lines 79-80, 311)

Windows Installer runs one MSI install per machine (error 1618). Use one queue for each
operation that changes the system (A11). The queue also gives the `queued` stage a meaning.

### I6 — Nit — a failed cancel is silent (line 444)

Show "cancel requested, installer finishing" and disable the button. This closes a WP5 gap.

### I7 — Nit — §0 names the mirrors (line 36)

Point to "the catalog backend chosen in the decision record".

## Notes from the main context

These notes compare the findings with research that the reviewers did not see
(`docs/adr/0001-community-catalog-backend.md`, written after commit `961b63d`).

- **I2, the official index:** the research measured it. The index has no publisher name,
  description or version list, and its format has no public contract. ADR 0001 rejects it for
  the rule "public, documented interfaces only". The part of I2 that removes the mirrors from
  §0 stands.
- **I1 and the COM path in I2 and I3:** ADR 0001 lists the COM API as option D. Access from an
  app with no package identity is not verified.
- **Z5:** ADR 0001 is the decision record that Z5 asks for. It recommends `winget search` and
  `winget show` through the executor.
- **Z2:** the research ran `winget search` and `winget show` on the `winget` source with no
  agreement flag and saw no prompt. That is one machine with earlier winget use; a new machine
  is not verified. It supports Z2 but does not prove it.
- **Z8:** `docs/designs/unattended-testing.md` now exists as a draft.
- **Z1 is correct and I caused it.** The status note and §6.6 were written in the same edit and
  the skill was not checked against §6.6.
