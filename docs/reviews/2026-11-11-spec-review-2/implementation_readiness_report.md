# Stage 9 — Implementation Readiness Test

- **Target:** `docs/SPEC.md` at commit `ce32d68`, DetailedRange lines 697-818 (§7, §8, §9),
  with §5.4, §6.6 and §6.7 as the reference for the WP5 work that remains.
- **Date:** 2026-11-12
- **Reviewers:** `egm-implementation-readiness` (implementation-readiness standard),
  `egm-codebase-details` (against branch `impl/6--install-loop`), `egm-style`. Each ran in a
  fresh context.
- **Raw totals:** 2 Blocker, 26 Major, 9 Nit. After the merge of related findings:
  2 Blocker, 13 Major, 1 Nit group.

Finding ids: `ready-1-n`, `code-1-n`, `style-9-n`.

## Verdict

Not ready for code, ready for the spike. An implementer can start the §6.6 spike today. The
code after the spike cannot start: the spike results do not select a dialog design (P1), and
the `consent` payload has no definition (P2). The description of the built code is correct:
the codebase reviewer found no built statement that the branch contradicts, except one
sentence in §8 (P3).

## Blockers

### P1 — The spike results do not select the work

- **Raw:** ready-1-1. **Lines:** 746-747, 751-752.
- The spike answers two questions. Nothing says what step 2 builds for each answer: a webview
  dialog or a native one, and what happens when winget gives no text and the fallback has no
  approval. "The consent tests of §6.6 pass" then has no fixed scope.
- **Recommendation:** add a decision table to WP5: spike result, dialog type, text source,
  tests that apply. State if WP5 can close while the fallback waits for approval.

### P2 — `options.consent` has no payload definition

- **Raw:** ready-1-2. **Line:** 747.
- No field names, types or serialization. The "shown version" has no meaning for
  `start_uninstall`, which installs nothing.
- **Recommendation:** give the JSON shape, for example
  `{ kinds: ["source" | "package"], version: string }`. State the uninstall rule.

## Major

### P3 — §8 states a guard that does not exist

- **Raw:** code-1-1, style-9-6. **Lines:** 797-799. **Code:** `tests/winget_smoke.rs:1,33`.
- §8 says in the present tense that the guard for a real winget test is in the test code. The
  one real test has only `#[ignore]`. The text does not say what the guard detects.
- **Recommendation:** mark the sentence *planned*, owner F2, and name the condition or point
  to the design section that defines it.

### P4 — The WP5 change list omits most of the code that changes

- **Raw:** ready-1-3, code-1-3, code-1-4, code-1-6, code-1-7, code-1-13.
- The list names `AppConfig` only. The flag also runs through `ipc/state.rs` and
  `WingetOptions.accept_agreements` into six argument builders, three of them read-only.
  `PackageExecutor::remove` takes no request, so consent on uninstall changes the trait, its
  two implementations, the command and the UI client. `finish()` in `winget_cli.rs` and
  `settle()` in `services/install.rs` need a new case. Nine tests and three skill lines
  assert the behavior of today.
- **Recommendation:** list each path that WP5 creates or changes, and the tests to rewrite.

### P5 — The UI cannot see which agreement is missing

- **Raw:** code-1-2. **Lines:** 544-545, 586-587. **Code:** `error.rs:98-107`,
  `ui/src/lib/types.ts:76`.
- `AgreementsRequired` has the two fields, but `Serialize` sends only `code`, `message`,
  `detail` and `retryable`.
- **Recommendation:** add the serialized fields and the UI type to the WP5 list.

### P6 — No command tells the core about a decline

- **Raw:** code-1-5. **Lines:** 561-562.
- A decline occurs in the UI only. `services/install.rs` cannot write the "agreements
  declined" row.
- **Recommendation:** add a planned command to §5.4, or remove the row from the design.

### P7 — The version rule rests on an unchecked exit code

- **Raw:** code-1-9. **Lines:** 555-556. **Code:** `winget_cli.rs:904`, `winget_parse.rs:351`.
- `PackageNotFound` carries the winget id and no version. Nobody checked which exit code
  winget gives for a `--version` that does not exist.
- **Recommendation:** add the exit code as a spike question. Add the version to the message.

### P8 — The gap row "cancel that cannot stop the tree" has no test and no detail

- **Raw:** ready-1-4, code-1-12, style-9-3. **Lines:** 748, 751-753.
- No detection condition, no event payload, no acceptance criterion. The progress call that
  the fix can reuse drops events when the channel is almost full. The criterion "a cancelled
  install leaves the ledger in its rest state" is unclear when winget completes after the
  cancel.
- **Recommendation:** define the condition, the event (stage and message) and the UI result.
  Add a test against the fake winget.

### P9 — The acceptance criteria do not cover five WP5 changes

- **Raw:** ready-1-5, code-1-8. **Lines:** 751-753.
- No check for: the CSP value, the removal of `accept_source_agreements`, the decline row,
  the UI read of `error.code`, the skill update. `FakeExec::install` ignores its request, so
  a service test cannot check the flags of a retry.
- **Recommendation:** one check for each change. Say that the flag test runs against the
  subprocess fake, or make `FakeExec` record the request.

### P10 — WP5 has no defined test level for its UI work

- **Raw:** ready-1-6, code-1-11. **Lines:** 800-806.
- The consent dialog is UI behavior. §8 sends UI tests to a design that is not approved. The
  UI also marks the operation as failed before the dialog can open, because the terminal
  progress event arrives first.
- **Recommendation:** name the test level and tool for the WP5 UI items now, or state that
  they do not gate WP5. Name the two UI handlers that change.

### P11 — Guardrails 3, 4 and 6 cannot be checked

- **Raw:** ready-1-7, style-9-4. **Lines:** 810-818.
- No values for timeouts and backoff. "Never silently pre-accepted" names no test. The list
  omits the CSP, text-only catalog data and no stored consent. Guardrail 6 does not use the
  term *operation*, so a retry may or may not need its own row.
- **Recommendation:** tie each item to a named test or a condition that a search can check.

### P12 — WP6 and WP7 omit decisions that the feature part gives them

- **Raw:** ready-1-8, ready-1-9, style-9-1, style-9-2. **Lines:** 755-768.
- WP6 omits the executor decision of §2.2, the operation queue and the one-instance rule.
  Its acceptance assumes that a pin blocks updates, and the entry says that is not decided.
  "Skipped" has no condition. WP7 omits the winget sources for read-only calls, the merge
  priority rule and consent for the `msstore` source.
- **Recommendation:** add each item to the "define before code" list of its work package.

### P13 — Acceptance criteria that a test cannot decide

- **Raw:** style-9-5, style-9-7, style-9-8, style-9-9, style-9-10. **Lines:** 734, 737,
  765-767, 772, 781-786.
- WP4 says "a real install", against §8. WP4 names "the commands that §5.4 listed at that
  time". WP7 "resolvable" has no definition, and "the branch" has no name. WP8 and WP11
  criteria have no pass condition; "machine-scope" and "admin-scope" are two terms for one
  thing; import does not say how it satisfies hard rule 4.
- **Recommendation:** list the WP4 commands by name and use the fake winget there. For WP7 to
  WP11, write a criterion that a test can decide, or state that the work package defines it
  before code.

### P14 — Existing code that the plan does not name

- **Raw:** code-1-10, code-1-14.
- Tests for the two agreement exit codes exist (`tests/executor_winget.rs:346-363,
  621-633`). `ProgressParser::agreements_shown()` detects license text and has no caller.
- **Recommendation:** name the two as the start points for the tests and for spike
  question 1.

### P15 — The order does not say if WP6 waits for F1

- **Raw:** ready-1-11, ready-1-12. **Lines:** 697, 710, 746.
- The gap table gives the version-read fix to "the F1 fix", and WP6 builds on it. The spike
  runs "on a real machine" and does not limit itself to read-only commands.
- **Recommendation:** state the dependency. Limit the spike to read-only commands, or require
  a sandbox.

## Nit group

### P16

- **Raw:** ready-1-10, style-9-11.
- Lines 752-753 repeat the §6.7 table. Wording items at lines 697, 709, 715, 731, 753, 759,
  797, 813, 815, 816: see style-9-11 in the reviewer output.

## Notes from the main context

- **P1, P2, P6 and P7 are design gaps, not wording.** P6 and P7 show that two parts of the
  §6.6 flow that this session wrote have no route in the code: the decline row and the
  version error.
- **P4, P5 and P14 are facts from the code.** They make the WP5 list longer, but they need
  no decision.
- **P10 depends on `docs/designs/unattended-testing.md`**, which waits for its own review.
- **P12 and P13 are about WP6 and later.** They do not block the WP5 work.
- Stage 9 permits one re-audit.
