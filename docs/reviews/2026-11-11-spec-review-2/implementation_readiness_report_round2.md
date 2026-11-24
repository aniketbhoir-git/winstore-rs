# Stage 9 — Implementation Readiness Test, round 2

- **Target:** `docs/SPEC.md` at commit `a0682b8`, DetailedRange lines 759-921 (§7, §8, §9),
  with §4.2, §5.4, §6.6 and §6.7 as the reference for the WP5 work that remains.
- **Date:** 2026-11-12
- **Reviewers:** `egm-implementation-readiness`, `egm-codebase-details` (against branch
  `impl/6--install-loop`), `egm-style`. Each ran in a fresh context.
- **Raw totals:** 1 Blocker, 17 Major, 9 Nit. After the merge of related findings:
  1 Blocker, 11 Major, 1 Nit group.
- This is the one re-audit that Stage 9 permits. No more audit of this stage follows.

Finding ids: `ready-2-n`, `code-2-n`, `style-2-n`.

## Verdict

Not ready for the WP5 core work. The spike can start for question 3 and for the read-only
part of question 1. Question 2 cannot run under the rules of the spec (Q7).

Round 1 status: P1 to P7, P9, P10, P12 to P14 and P16 are closed. P8 and P11 are partly
closed (Q6, Q9). P15 is closed for the WP6 order; its spike limit made a new conflict (Q7).

The description of the built code is correct. The codebase reviewer found no *built* or
*built on branch* statement that the branch contradicts, and all 11 file rows of the WP5
table name real files, types and functions.

## Blocker

### Q1 — The dialog has no source for its data

- **Raw:** ready-2-1. **Lines:** 553-556, 569-571, 392-421, 624-636.
- In cases (a) and (b) the dialog shows the agreement text or link, the version and the
  winget source. No command, error field or detail field carries these to the UI. The
  `agreements_required` JSON has only the two kind fields. `target_version` is cleared when
  the operation ends (line 685), so the UI cannot learn the version that winget resolved.
- **Recommendation:** name the source of each dialog field (new error fields or a new
  command with a row in §5.4) and add it to the file table. Name the `PackageDetail` field
  that holds the link for case (b).

## Major

### Q2 — The update builder takes flags from a request that does not exist

- **Raw:** ready-2-6, code-2-1, style-2-5. **Lines:** 370-372, 628-629.
  **Code:** `executor/mod.rs:76`, `executor/winget_cli.rs:645-647`, `ipc/commands.rs:160`.
- `PackageExecutor::update(package)` has no request, and the table changes only `install`
  and `remove`. Consent exists only on `start_install` and `start_uninstall`.
- **Recommendation:** state that `update_args` passes no agreement flag until WP6 defines
  update consent, or give `update` a request.

### Q3 — Two version fields with no rule

- **Raw:** ready-2-2, style-2-11. **Lines:** 330-337, 397, 572-576, 646-655.
- `start_install` gets `options.version` and `options.consent.version`. Nothing says which
  one wins, or what the core passes when the consent kinds are `["source"]` and the version
  is `null`. No test covers `invalid_config` for a `package` kind with no version.
- **Recommendation:** state the rule and add the test.

### Q4 — Case (c) does not say what the core builds

- **Raw:** ready-2-3, style-2-2, style-2-3, style-2-10. **Lines:** 555-561, 566-571, 611,
  646-655, 812, 824.
- The text names no actor and no error code for the message "agreement text is not
  available", and flow step 3 says the UI shows no error after `agreements_required`. It
  does not say if `start_install` accepts `consent` in case (c). "All consent tests" and
  "the tests for Accept" have no definition, so the WP5 acceptance cannot be decided for
  case (c). Line 812 says the spike picks the case; the maintainer's approval decides
  between (b) and (c).
- **Recommendation:** add a "Core" column to the decision table. Tag each test in "Tests to
  add" with the cases it applies to. Say that the spike decides (a) or not (a).

### Q5 — Consent on uninstall: one branch and one site are missing

- **Raw:** ready-2-4, code-2-2. **Lines:** 338-340, 540, 630.
  **Code:** `services/install.rs:133-141`.
- Nothing says what to build when spike question 3 shows that `winget uninstall` accepts no
  agreement flag. The file table omits `start_uninstall(core, sink, id)` in
  `services/install.rs`, which takes no options, and `services/install/tests.rs`.
- **Recommendation:** state the result for each answer of question 3. Add the two sites.

### Q6 — The "cancel failed" event has no marker, no send method and no test seam

- **Raw:** ready-2-5, code-2-7. **Lines:** 741, 816.
  **Code:** `executor/winget_cli.rs:357, 437-444, 838-849`.
- The UI can find the event only by a match of the message text. `kill_tree` is a free
  function, so "a stop function that fails" needs an injection point. A send that is never
  dropped can take the slot kept for the terminal event, or stall the read loop.
- **Recommendation:** add a field to `ProgressEvent` (§4.2), name the injection point, and
  define the send.

### Q7 — Spike question 2 needs a command that the spec forbids on a developer machine

- **Raw:** ready-2-7. **Lines:** 539, 542-544, 887-890.
- `winget install --version <missing>` is a command that can install. Question 1 can need
  an install run also. Such commands run in CI or a sandbox only, and neither exists before
  the testing design is approved.
- **Recommendation:** name how to run these commands (a manual CI workflow or Windows
  Sandbox with steps), or reduce the questions to commands that cannot install.

### Q8 — The list of tests to rewrite is wrong in two files and has no names

- **Raw:** code-2-3, code-2-4, code-2-5, style-2-8. **Lines:** 626, 638-640.
- 7 of 9 are correct. `tests/install_loop.rs` has no test of the config rule; only the
  fixture `Fx::new` sets the field. In `config.rs` only `default_has_agreements_off` asserts
  the rule; `modified_round_trips` needs an edit, not removal. Line 626 says "removed" and
  line 639 says "rewritten".
- The tests that assert the config rule: `agreement_flags_only_when_accepted`,
  `pin_agreements_flag_only_when_accepted` (`executor/winget_cli.rs`);
  `agreements_flags_follow_config`, `agreement_flags_on_update_remove_list_and_pin`,
  `list_updates_parses_table_and_skips_unmatched_rows` (`tests/executor_winget.rs`);
  `install_then_remove_one_small_package` (`tests/winget_smoke.rs`);
  `default_has_agreements_off` (`config.rs`).
- **Recommendation:** write the names into the spec, with a separate note for fixture edits.

### Q9 — Guardrail 6 has no check and contradicts §6.7

- **Raw:** style-2-1, code-2-8. **Lines:** 903, 916-918 against 725, 732-733, 746, 753.
- It requires one `operation_log` row for each operation. §6.7 adds Warn rows and an Error
  row. Line 903 says each item names its check.
- **Recommendation:** "one outcome row for each operation; Warn and Error rows can be
  added", with a named test.

### Q10 — `pin` is read-only in one place and a change in another

- **Raw:** style-2-4. **Lines:** 629 against 375.
- That leaves the test "a read-only call never passes a flag" (line 651) unclear.
- **Recommendation:** one classification, used in the two places.

### Q11 — "The fake winget" names two different things

- **Raw:** style-2-6. **Lines:** 822, 842 against 647-648, 883.
- It can be the subprocess fixture or `FakeExec`, and `FakeExec` ignores its request.
- **Recommendation:** name the fixture that each criterion needs.

### Q12 — The WP5 status does not show its dependency on the testing design

- **Raw:** style-2-7, ready-2-8 (owner). **Lines:** 8-12, 774, 816-818, 892-893.
- WP5 cannot close before `docs/designs/unattended-testing.md` is approved. The status note
  and the WP5 status row do not say so. No owner is named for the component-test level.
- **Recommendation:** add the dependency and the owner (F2, #21) to the two places.

## Nit group

### Q13

- **Raw:** code-2-6, style-2-9, ready-2-8 (file).
- The skill line list omits lines 52, 54 and 161 of `.claude/skills/winget-executor/SKILL.md`.
- The status table shows WP7 as "Not started"; its button and `open_store_page` are *built
  on branch*.
- The CSP test has no named file.

## Notes from the main context

- **Q1, Q7 and Q8 come from text that this session wrote.** Q1 is a gap in the §6.6 design.
  Q7 is a conflict between two round 1 fixes (P7 and P15). Q8 is the list that the session
  did not check line by line.
- **Q1, Q3, Q4, Q5 and Q7 need a decision.** The others are facts or wording.
- **Q2 to Q5 depend in part on the spike.** A fix can give a rule for each spike result; it
  cannot remove the dependency.
- The readiness round found design gaps two times. A third round of text fixes with no
  spike result has low value: the spike changes §6.6 again.
