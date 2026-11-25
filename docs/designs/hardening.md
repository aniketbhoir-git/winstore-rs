# Design — hardening: elevation, toasts, scheduled scan, import/export (WP11)

- **Status:** Draft 2026-11-25 — GTAVI mode; needs review before code. Run
  `/egm:egm-review docs/designs/hardening.md` before code, if the owner wants a review.
- **Date:** 2026-11-25
- **Issues:** #12 (SPEC §7 WP11), review actions A9, A12 and A24.
- **Depends on:** #6 (install loop, merged or on branch), F4 (#45, consent case (b)) and
  three WP6 items: the operation queue (A11), one app instance, and `check_updates` with
  `--source winget` (SPEC §6.7 gap table). The steps that need them say so.
- **Owner gates:** step 8 (Task Scheduler) and the SPEC edit that replaces "Task Scheduler"
  in WP11. See "Owner questions".

## Objective

WP11 adds the Windows platform glue. SPEC §7 WP11 says: "Before code, WP11 must add a flow
for each item, with criteria that a test can decide. The import flow must show how one user
click for a list of packages satisfies hard rule 4." This document gives these flows.

Acceptance of #12, made testable:

1. **Elevation.** A machine-scope install passes `--scope machine` to winget. When the
   installer needs administrator rights and does not get them, the operation ends with
   `elevation_required`, and the UI shows a text that tells the user what to do. An L1 test
   with the subprocess fake proves the flag and the error code.
2. **Toast.** When a scheduled scan finds a new set of updates, the app shows one toast. An
   L1 test with a fake notifier proves it. The test uses no real winget and no real toast.
3. **Import.** The app reads a `winget export` file and shows each package in a review list.
   Only the packages that the user ticks one by one go into the install queue. An L1 test
   with `FakeExec` proves that no other package starts.

## Non-goals

- An install of a local MSIX file. It is deferred (SPEC §2.1, review action A9). The backlog
  entry of #12 still names it; see "Owner questions".
- An own UAC helper: a second process, a service, or a `runas` relaunch of winget. The app
  never starts an elevated process itself. Windows shows the UAC prompt of the installer.
- Automatic updates. A scan only lists updates. `is_auto` stays reserved (SPEC §4.2).
- A Windows service or a task that runs as `SYSTEM`.
- A tray icon and a "run in the background" mode.
- Action buttons in a toast ("Update now"). A toast opens the app and does nothing more.
- `winget import` as a command. The app reads the file itself (section 4).
- An own file format for import and export.
- A real UAC prompt, a real toast, or a real scheduled task in a test.

## Overview

```
UI ──start_install(scope?)──▶ services/install.rs ──▶ WingetCli: winget install --scope S
                                                          │ installer asks Windows for UAC
                                                          ▼
                                           exit 0x8A150019 / 740 ──▶ elevation_required

services/schedule.rs ──timer──▶ check_updates (read-only) ──▶ Notifier::show(toast)
                                                                  (platform/windows.rs)

UI ──export_packages──▶ WingetCli: winget export ──▶ file in the exports folder
UI ──read_import_file──▶ services/import.rs: parse + check ──▶ review list (no install)
UI ──start_import(ticked ids)──▶ operation queue (WP6) ──▶ one start_install per package
```

`platform/windows.rs` holds the code that calls Windows APIs: the elevation check of the app
process, the toast, and (step 8) the scheduled task. Each part is behind a small trait, so
tests use a fake.

## Design

### 1. Elevation

**How winget elevation works.** Microsoft Learn, "Administrator considerations" of the
winget overview and "Scope for specific user vs machine-wide" of the winget troubleshooting
page, says:

- When winget runs with no administrator rights, some installers need elevation. Windows
  shows the UAC prompt when the installer starts. When the user refuses, the install fails.
- When winget runs as administrator, the user sees no UAC prompt.
- `--scope user|machine` asks for a scope. MSIX packages follow it reliably. MSI packages
  follow it in most cases. An EXE installer can ignore it. It can pick a scope from the
  membership of the Administrators group. A user-scope package can still need UAC.

So the app does not know before the run whether an install needs a UAC prompt. The design
follows from this.

**Rules.**

1. The app runs as the user who started it (manifest level `asInvoker`). It never asks for
   elevation for itself.
2. winget runs as a child of the app with the same token. An installer that needs
   administrator rights asks Windows; Windows shows its own UAC prompt. The app shows no
   prompt of its own and does not simulate one.
3. `start_install` takes `scope` (`user`, `machine` or none). This option exists in the code
   (`InstallScope`, `ipc/commands.rs`). The detail page gets a scope choice: "Default",
   "Only for me" (`user`), "For all users" (`machine`). "Default" passes no `--scope` flag,
   so winget decides. This is the behavior of today (see "Decisions" 1).
4. Uninstall and update pass no scope. winget uses `--scope` there as a filter of the
   installed scope only (`winget-executor` skill), and the ledger does not store a scope.
5. Exit codes. The map exists in `executor/winget_parse.rs`: `0x8A150019`
   (`COMMAND_REQUIRES_ADMIN`) and `740` (`ERROR_ELEVATION_REQUIRED`) give
   `ElevationRequired`. The installer code for a refused UAC prompt (often `1223`,
   `ERROR_CANCELLED`) and the way winget reports it are **not verified** (see "Not
   verified"). Until a recorded fixture proves them, a refused prompt ends as
   `winget_failed`, as today.
6. The UI text for `elevation_required`: "This app needs administrator rights to install.
   Windows did not give them. Try again and accept the Windows prompt, or choose 'Only for
   me' if the app supports it." The text names no other action.
7. When the app process itself is elevated (the user started it as administrator), installers
   show no UAC prompt. The app then shows a banner: "Store runs as administrator. Installers
   will not ask before they change the system." `platform/windows.rs` reads
   `TokenElevation` with the `windows` crate; the `Win32_Security` feature is already on.
   The banner only informs; the app does not refuse to run.

**Cancel of an elevated installer (A12).** A process with a normal token cannot stop an
elevated child. The built path stays: the stop fails, the executor emits one
`store:progress` event with `cancel_failed` = true, and the UI disables Cancel (SPEC §6.7,
"Cancel"). WP11 adds one UI text for this case: "The installer runs with administrator
rights. Store cannot stop it." The stage-dependent cancel of A12 for a normal installer is
not elevation work. It goes to the WP6 queue work (see "SPEC edits needed").

### 2. Toast notifications

**Options.**

| Option | For | Against |
|---|---|---|
| (a) `tauri-plugin-notification` | Official Tauri plugin; a JavaScript and a Rust API. | A new crate and a new npm package, outside SPEC §2. A plugin permission in the capability file. On Windows it works only for an installed app; in development it shows the name and icon of PowerShell (Tauri v2 docs). |
| (b) The `windows` crate, `Windows.UI.Notifications` | SPEC §2 names this crate for notifications. No new crate; two more features of `windows`. The core sends the toast, so the webview needs no permission. | More code than (a). It also needs an AppUserModelID, which the installer registers. |

**Decision: (b).** It stays inside SPEC §2 and gives the webview no new permission. The code
is behind a trait, so a change to (a) touches one file (see "Decisions" 3).

**Rules.**

1. `platform/windows.rs` defines `trait Notifier { fn show(&self, toast: &Toast) }`. The real
   one builds the toast XML and calls `ToastNotificationManager`. Tests use `FakeNotifier`,
   which records each toast.
2. A toast has a title and a body of plain text. The text comes from the app only. A package
   name from a catalog is escaped as XML text. A toast has no image from the network, no
   link and no action button. A click opens or focuses the app window.
3. The app shows a toast in two cases only:
   - a scheduled scan finds a set of updates that differs from the set of the last toast;
   - an operation ends while the app window does not have focus.
4. A setting turns toasts off (`notifications_enabled`, default on). Windows settings ("Do
   not disturb", the per-app switch) also apply; the app does not read or change them.
5. No telemetry. A toast makes no network call and the app records nothing about it, except
   one Info row in the trace log.
6. When the toast call fails (for example, no AppUserModelID in a development build), the
   app writes one Warn row to the trace log and continues. A failed toast never fails a scan
   or an operation.

### 3. Scheduled update scan

**Options.**

| Option | Privilege | Reverse | Runs when the app is closed |
|---|---|---|---|
| (a) A timer in the app while it runs (tokio) | The user, no system change | Turn off the setting; nothing stays on the system | No |
| (b) A per-user task in Task Scheduler, run level `LIMITED`, "only when the user is logged on" (`/it`) | The user, no password stored; one task entry in the user's Task Scheduler folder | Turn off the setting; the app deletes the task. An uninstall of the app must also delete it | Yes |
| (c) A task that runs as `SYSTEM` or a Windows service | Administrator | Needs elevation to remove | Yes |

(c) is rejected: it needs administrator rights and is hard to reverse.

**Decision: (a) first, (b) as an opt-in later step behind an owner gate.** (a) has the least
privilege and leaves nothing on the system. SPEC §7 WP11 and the backlog say "Task
Scheduler". This design changes that, so it is an owner question. Step 8 builds (b) only when
the owner confirms it.

**Flow of a scan (both options).**

1. The setting `update_scan` is `off`, `daily` or `weekly`. Default: `off`. The user turns it
   on in the settings page.
2. `services/schedule.rs` holds `ScanScheduler`. While the app runs, it checks every 15
   minutes whether a scan is due. A scan is due when the last scan is older than the
   interval. The time of the last scan and the set of package ids of the last toast are in
   the `settings` table.
3. A scan calls `check_updates()`. It is read-only (SPEC §6.3): `winget upgrade` with
   `--source winget`, no package, no `--all`, and no agreement flag. A scan never starts an
   install, an update or an uninstall.
4. A scan does not run while an operation is active, and it waits for the WP6 queue. When it
   is due during an operation, it runs after the queue is empty.
5. Result:
   - Updates found, and the set of ids differs from the last toast: one toast, "N updates
     are available". The update page lists them.
   - Updates found, same set: no toast.
   - No updates: no toast.
   - `agreements_required`: one toast, "Open Store to review an agreement". No flag is
     passed and no consent is stored (SPEC §6.6, "Read-only calls").
   - Another error (`winget_not_found`, `source_unavailable`, a time-out): no toast; one Warn
     row in the trace log. The next scan comes at the next interval.

**Option (b) details (step 8, owner-gated).**

- The app registers the task with `schtasks.exe /create /xml <file> /tn "Store\UpdateScan"
  /f`. The XML sets the principal to the current user, `LogonType` `InteractiveToken`,
  `RunLevel` `LeastPrivilege` (the `/rl LIMITED` of Microsoft Learn, "schtasks create"), and no
  stored password. The action starts `store.exe --scan-updates`.
- `--scan-updates` starts the app with no window. With one app instance (WP6), a running
  instance gets the argument and does the scan; otherwise the new instance does the scan,
  shows the toast if any, and exits.
- Turn off: `schtasks.exe /delete /tn "Store\UpdateScan" /f`. The installer of the app deletes
  the task on uninstall.
- The runner is a trait (`TaskRegistrar`). Tests use a fake that records the argument list.
  No test runs `schtasks.exe`.

### 4. Import and export

**Format.** The app uses the JSON format of `winget export` and `winget import` (Microsoft
Learn, "export command" and "import command"; schema in the `schemas/JSON/packages` folder of
`microsoft/winget-cli`). The file has `Sources`, each with `SourceDetails` and `Packages`;
each package has `PackageIdentifier` and an optional `Version`. A file that Store writes can
go to plain winget, and the reverse. An own format gives nothing more (see "Decisions" 5).

**Export.**

1. Command `export_packages`. It runs `winget export -o <temp file> --source winget
   --disable-interactivity` through `WingetCli`. It passes no agreement flag, although
   `--accept-source-agreements` is valid on export. When winget needs a source agreement,
   the command returns `agreements_required`, and nothing is written.
2. `winget export` is read-only on the system. It lists the installed packages that the
   `winget` winget source knows, not only the packages that Store installed.
3. The app parses the temp file with the import parser (below). When it parses, the app
   copies it to the exports folder, `Documents\Store\exports\store-export-<date>.json`, and
   shows the path with an "Open folder" button. When it does not parse, the command returns
   `winget_failed` and keeps no file.
4. Versions: export passes no `--include-versions`. An import then installs the latest
   version, as winget does by default.

**Import: the file.** The file is untrusted input.

1. Command `read_import_file(path)`. The path comes from a file that the user drops on the
   import page (the Tauri drag-and-drop event of the core) or from the list of files in the
   exports folder. No new dialog dependency (see "Owner questions").
2. `services/import.rs` parses with `serde` into a strict subset of the schema. Limits: file
   size 1 MiB, 500 packages. Larger gives `invalid_config`.
3. Each `PackageIdentifier` must pass the same id check as a search result and
   `guard_args()` (no leading `-`, no space, no control character). A failed id is shown as
   "not valid" and cannot be ticked.
4. A package of a winget source other than `winget` is shown as "source not supported" and
   cannot be ticked. A paid item cannot be ticked (hard rule 1).
5. A package that the ledger or `winget list` shows as installed is shown as "Installed" and
   cannot be ticked. Import does not update.
6. Duplicate ids are merged into one row. The `Version` field is shown and not used: the
   install takes the latest version (see "Decisions" 6).
7. The text of the file is shown as text only, never as HTML (SPEC §6.6, "Trust boundary").
8. `read_import_file` changes nothing on the system and starts no winget process except
   `winget list`.

**Import: the review list and hard rule 4.** Hard rule 4 needs an explicit user click for
each install. The flow:

1. The review list shows one row per package. Each row has its own tick box. No box is ticked
   when the list opens. There is no "select all" (see "Owner questions").
2. The button "Install N selected" is disabled while N is 0. Its label always shows N.
3. A tick on a row is the confirmation for that package. The button click starts the queue.
   So each install comes from one user tick on that package plus one click that the user
   sees with the count. This is how one click for a list satisfies hard rule 4.
4. `start_import(package_ids)` checks each id against the last review list of this app
   session. An id that was not tickable, or not in the list, gives `invalid_config`, and
   nothing starts. The core does not trust the webview for the tick rules.
5. Each id becomes one normal install in the WP6 queue, one at a time. Each install has the
   same rules as a click on Install: paid check, ledger check, consent check.
6. **Agreements.** `start_import` takes no consent and passes no agreement flag. When a
   package needs an agreement, its install ends with `agreements_required`; the queue skips
   it and continues with the next one. The review list marks the row "Needs your review".
   A click on the row opens the consent dialog of F4 for that package alone. Only an
   "Accept" there gives the `VerifiedConsent` that lets one agreement flag pass for one
   install (SPEC §6.6, "How consent is bound").
7. Cancel stops the current install (SPEC §6.7) and removes the rest of the import from the
   queue.
8. When the queue ends, the app emits one `store:import-summary` event: `total`,
   `succeeded`, `failed`, `skipped`, `needs_review`, and one outcome per package.

## Security notes

- **No own elevation.** The app never calls `ShellExecuteEx` with `runas` and never starts
  an elevated process. Windows shows each UAC prompt. The prompt names the installer, not
  Store.
- **An elevated app.** When the user starts Store as administrator, installers do not ask.
  The banner of section 1 rule 7 says so.
- **Untrusted import file.** Size and count limits, a strict schema subset, an id check, no
  shell, and text-only display. An id never reaches winget as a flag (`guard_args()`).
- **No agreement flag from import, export or a scan.** The only way to an agreement flag stays
  the F4 consent path for one install.
- **No telemetry; hard rule 5.** Toasts, the timer, the scheduled task and import make no
  network call of their own. Only winget makes calls.
- **The scheduled task (step 8).** Run level limited, interactive token, no stored password,
  one task in the user's folder, removed when the setting goes off and on uninstall.
- **Toast text.** Plain text, escaped as XML. No remote image, no link.
- **A new IPC command** needs a row in SPEC §5.4 and a capability entry. The commands of this
  design are `export_packages`, `read_import_file`, `start_import` and the settings commands.

## Tests

All tests run unattended through `scripts/test.ps1`, with no network, no real winget, no
real UAC prompt, no real toast and no real scheduled task (`docs/designs/unattended-testing.md`,
L1 and L2). No test calls `WingetCli::connect` or runs `schtasks.exe`.

| Test | Level | Where | What it checks |
|---|---|---|---|
| Scope flag | L1 | `winget_cli.rs` unit tests | `--scope user` and `--scope machine` in the install args; none for "Default", uninstall and update. |
| Elevation exit codes | L1 | `tests/install_loop.rs`, subprocess fake | Exit `0x8A150019` and `740` give `elevation_required`; the ledger goes to the rest state (SPEC §6.7). |
| Elevated app check | L1 | `platform/windows.rs` | The `ElevationCheck` trait; a fake gives true and the status command reports it. |
| Elevation text | L2 | `ui/` Vitest | `elevation_required` shows the text of section 1 rule 6. The banner shows when the status says elevated. |
| Cancel text | L2 | `ui/` Vitest | `cancel_failed` shows the text for an elevated installer. |
| Toast XML | L1 | `platform/windows.rs` | A name with `<`, `&` and `"` is escaped. No image, link or action element. |
| Toast on finish | L1 | `ipc/tests.rs`, `FakeExec`, `FakeNotifier` | Window not focused: one toast. Focused: none. Setting off: none. |
| Scan timer | L1 | `tests/schedule.rs`, `tokio::time::pause` | Setting off: no scan. Due: one `check_updates`. Not due: none. An active operation delays the scan. |
| Scan result | L1 | same file | New set: one toast. Same set: none. Empty: none. `agreements_required`: the review toast, and no agreement flag in the argv file. Error: no toast, one Warn row. |
| Scan never installs | L1 | same file | `FakeExec` records no install, update or uninstall call during a scan. |
| Export args | L1 | `winget_cli.rs` unit tests | `export -o <path> --source winget --disable-interactivity`; no agreement flag. |
| Export file | L1 | `tests/import_export.rs`, subprocess fake | The fake writes a recorded export file; the app copies it to the exports folder. A bad file: `winget_failed`, no file kept. |
| Import parser | L1 | `services/import.rs` unit tests | Recorded export file; with `Version`; other source; duplicate ids; id `--accept-package-agreements` refused; 501 packages and 1 MiB + 1 byte refused; bad JSON. |
| Import tick rules | L1 | `ipc/tests.rs`, `FakeExec` | Only ticked ids start. An id not in the list, an installed one, a paid one or a bad one gives `invalid_config` and nothing starts. |
| Import agreements | L1 | `tests/import_export.rs`, subprocess fake | Package 2 of 3 exits `0x8A150041`: 1 and 3 install, 2 is `needs_review`; the argv file holds no agreement flag. |
| Import cancel | L1 | same file | Cancel during package 1: 2 and 3 do not start. |
| Review list | L2 | `ui/` Vitest, `mockIPC` | No box ticked at open; button disabled at 0; label shows N; untickable rows have no box. |
| Scheduled task XML (step 8) | L1 | `platform/windows.rs` | `LeastPrivilege`, `InteractiveToken`, no password element, the action `--scan-updates`. |
| Task register (step 8) | L1 | same | Fake `TaskRegistrar`: on gives `/create ... /f`; off gives `/delete ... /f`. |
| No real schtasks (step 8) | L1 | `tests/no_real_schtasks.rs` | No test file other than the fake calls `schtasks`; same method as `tests/no_real_winget.rs`. |

Fixtures: `src-tauri/tests/fixtures/winget/1.29/export-small.json` (recorded once from
winget 1.29 on a GitHub-hosted runner) and hand-made files for the bad cases. The IPC
payloads of the new commands go in `src-tauri/tests/fixtures/ipc/` for L2.

**Open checks (not L1 or L2).** These stay in "Hand-over from SPEC §8" of
`docs/designs/unattended-testing.md`, label `needs-windows-test`:

- A real UAC prompt and a refused prompt. A GitHub-hosted runner runs as administrator, so it
  shows no prompt. A person checks this in Windows Sandbox.
- A real toast from an installed build.
- A real scheduled task (step 8).

## SPEC edits needed

These edits go in the same pull request as the code step that makes them true.

| Section | Edit |
|---|---|
| §1.3, errors | `ElevationRequired` gets its message (section 1 rule 6). |
| §2.1 | The local MSIX row stays deferred; add "not part of #12". |
| §3, layout | Add `services/import.rs`. `platform/windows.rs`: elevation check, toast, scheduled task (step 8). |
| §4.3, `settings` | Keys `notifications_enabled`, `update_scan`, `update_scan_last`, `update_scan_last_ids`. |
| §5.4 | Rows for `export_packages`, `read_import_file`, `start_import`, the settings commands, and the event `store:import-summary`. `start_install`: "the UI sets `scope` from WP11". |
| §6 | New §6.8 Elevation, §6.9 Toasts, §6.10 Scheduled scan, §6.11 Import and export, from sections 1 to 4 of this design. §6.11 states the hard-rule-4 argument (tick per package plus one counted click). |
| §6.7, gap table | A24 scope row: built by steps 1 and 2. A12 row: the elevated part is closed by the `cancel_failed` path; the stage-dependent part moves to WP6. |
| §7, WP11 | Replace "(Task Scheduler)" with "(an in-app timer; Task Scheduler opt-in after an owner decision)". Add the acceptance criteria of "Objective". Remove "No item has a flow in §6". |
| §8 | Add the scan, toast and import tests; the open checks above. |

Outside SPEC: `docs/ISSUE-BACKLOG.md` issue 12 (MSIX moved out, acceptance text),
`CLAUDE.md` status, the `winget-executor` skill (`export` args; refused-UAC code when
verified), `docs/designs/unattended-testing.md` hand-over table (elevation row).

## Decisions

Each choice of taste takes the option that is easiest to reverse.

1. **Default scope: no `--scope` flag.** This is the behavior of today. A24 proposes user
   scope as the default; see "Owner questions". Reverse: one default value in the UI.
2. **No own UAC helper.** Windows prompts for the installer. Reverse: add a helper later;
   nothing depends on its absence.
3. **Toasts through the `windows` crate behind `Notifier`.** SPEC §2 names this crate.
   Reverse: one implementation of the trait.
4. **In-app timer first; Task Scheduler opt-in, owner-gated (step 8).** Least privilege;
   nothing stays on the system. Reverse: build step 8.
5. **winget's export JSON, no own format.** Files work with plain winget.
6. **Import installs the latest version.** The `Version` field is shown, not used. Reverse:
   pass `version` in the install options.
7. **Import does not call `winget import`.** That command installs the whole list in one run
   and cannot ask per package.
8. **Import skips a package that needs an agreement and continues.** Same rule as
   `update_all` for a package agreement (SPEC §6.3). The user reviews it with F4 later.
9. **No new dialog dependency.** Drop a file, or pick from the exports folder.
10. **Scan default off.** The user turns it on.

## Risks

| Risk | Mitigation |
|---|---|
| An EXE installer ignores `--scope`. The user picks "Only for me" and gets a machine install, or a UAC prompt. | The UI text says winget asks for the scope and the installer decides. The recorded version check (SPEC §6.7) still runs. |
| The code of a refused UAC prompt is not known. | It ends as `winget_failed` until a recorded fixture proves the code. |
| A toast fails in a build with no AppUserModelID. | Warn row; the scan and operation results stay in the app. |
| The in-app timer does not scan while the app is closed. | Documented; step 8 if the owner wants it. |
| An import file names many packages; a long queue. | 500-package limit; one install at a time; Cancel clears the rest. |
| The user starts Store as administrator and gets no UAC prompt. | Banner. |
| Import needs the WP6 queue, which does not exist yet. | Step 7 waits for it. Steps 1 to 6 do not need it. |

## Not verified

- The exit code that winget returns when the user refuses a UAC prompt (often installer code
  `1223`). Record it in Windows Sandbox before a mapping is added.
- Toast activation (click) for an app with no package identity: it needs the AppUserModelID
  from the installer. Check with the installed build.
- Whether `winget export` asks for a source agreement for the `winget` winget source. The
  real-winget check on a hosted runner records it with the fixture.

## Implementation plan

Each step is one pull request. Each step runs `cargo build`, `cargo clippy` and `cargo fmt`
in `src-tauri/`, and `powershell -File scripts/test.ps1` before the push. The table gives the
focused test command of the step.

| Step | Pull request | Test command |
|---|---|---|
| 1 | **Elevation core.** `ElevationCheck` in `platform/windows.rs` (token read). Scope flag tests. Status reports an elevated app. `ElevationRequired` message. SPEC §6.8. | `cd src-tauri && cargo test --lib executor platform` |
| 2 | **Scope UI.** Scope choice on the detail page, `elevation_required` text, admin banner, `cancel_failed` text. | `cd ui && npm test` |
| 3 | **Toasts.** `Notifier` trait, `windows` crate toast, `FakeNotifier`, toast on finish when the window has no focus, `notifications_enabled` setting. SPEC §6.9. | `cd src-tauri && cargo test --lib platform && cargo test --lib ipc` |
| 4 | **In-app scan.** `services/schedule.rs`, `update_scan` settings, the scan result rules, settings UI. Needs `check_updates` with `--source winget` (WP6 gap row). SPEC §6.10. | `cd src-tauri && cargo test --test schedule` |
| 5 | **Export.** `export_packages`, exports folder, recorded fixture, export tests. | `cd src-tauri && cargo test --test import_export export` |
| 6 | **Import read.** `services/import.rs` parser and checks, `read_import_file`, review list UI with tick boxes. Installs nothing. SPEC §6.11. | `cd src-tauri && cargo test --lib services::import` and `cd ui && npm test` |
| 7 | **Import install.** `start_import`, the queue use, agreement skip, cancel, `store:import-summary`. Waits for the WP6 queue (A11). | `cd src-tauri && cargo test --test import_export` |
| 8 | **Task Scheduler (owner-gated).** `TaskRegistrar`, task XML, `--scan-updates`, delete on off and on uninstall, `no_real_schtasks` test. Waits for one app instance (WP6) and the owner's "yes". | `cd src-tauri && cargo test --lib platform::task && cargo test --test no_real_schtasks` |

Steps 1 to 6 need no WP6 code except the `--source winget` row for step 4. Step 7 closes
the import acceptance of #12. Step 4 with step 3 closes the toast acceptance with the in-app
timer; step 8 closes it with Task Scheduler if the owner wants that.

## Owner questions

1. **Task Scheduler or in-app timer.** SPEC WP11 and the backlog say Task Scheduler. This
   design builds the in-app timer and puts the per-user `LIMITED` task in step 8. Build
   step 8, or drop it?
2. **Default scope.** A24 proposes user scope as the default. A user-scope default fails for
   packages that have only a machine installer. Keep "no flag", or use `user`?
3. **"Select all" in the import review list.** Not built: a tick per package is the
   confirmation. Is one "select all" click enough for hard rule 4?
4. **File dialog.** Add `tauri-plugin-dialog` (a new dependency outside SPEC §2) for open
   and save, or keep drop-a-file and the exports folder?
5. **Offline MSIX install.** The backlog entry of #12 names it; SPEC §2.1 defers it (A9).
   Remove it from #12 and open a separate issue?
6. **A "run as administrator" retry.** Not built. Is an own UAC helper ever in scope?
