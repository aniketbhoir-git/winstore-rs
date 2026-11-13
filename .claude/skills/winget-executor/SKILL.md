---
name: winget-executor
description: Implement or maintain the winget CLI executor (src-tauri/src/executor/winget_cli.rs, Issue #4) and the install loop (Issue #6) in the Store repo. Use for winget discovery, argument building, stdout/exit-code parsing, ProgressEvent stages, cancellation, the fake-winget test fixture, or any "winget exit code", "winget output", "install/upgrade/list parser", "PackageExecutor" question.
disable-model-invocation: false
---

# winget executor

Everything needed to build `executor/winget_cli.rs` without re-running winget discovery.
Facts were observed on winget 1.29.380 (Windows 11) unless marked INFERRED.
Raw help, sample output and the full exit-code table: `references/winget-cli.md`.
Contract: SPEC §5.2, §6.2, §6.3, §8.

## Hard rules

| Rule | Detail |
|---|---|
| User click starts every mutating op | install, remove, update, pin. Never from a timer or a catalog refresh (SPEC §0 rule 4). |
| Surface agreements, never pre-accept | Store passes no agreement flag in any command (WP5, case (c) of SPEC §6.6). When winget exits with an agreements code, the operation ends with `agreements_required`. |
| One op per package id | Acquire `OpGuard` (per-package mutex map, SPEC §1.2) before spawning. |
| Log every op | Write `operation_log` rows (start, each warn/error line, final exit status). Raw stderr goes in `detail`. |
| Never use `--force` or `--ignore-security-hash` | No code path, no config switch. |
| No real winget in unit tests | Use the fake fixture. Real winget runs only in CI or Windows Sandbox (SPEC §8), never on a developer machine. |

## Fixed facts

| Topic | Fact |
|---|---|
| Discovery order | 1) `STORE_WINGET_PATH` env var (tests/override), 2) `winget.exe` on PATH, 3) `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe`, 4) `Get-AppxPackage Microsoft.DesktopAppInstaller` then `InstallLocation\winget.exe`. None found -> `AppError::WingetNotFound`. |
| Version probe | `winget --version` prints `v1.29.380` (strip `v`). `winget --info` adds directories and admin settings. |
| Minimum version | Require >= 1.6.0 (project choice, not from docs: all flags below exist well before it). Lower -> `AppError::Unsupported("winget >= 1.6 required")`. Raise only on evidence. |
| Sources here | `msstore` (storeedgefd.dsx.mp.microsoft.com/v9.0), `winget` (cdn.winget.microsoft.com/cache), `winget-font` (explicit). Always pass `--source` explicitly. |
| `--disable-interactivity` | Always pass. A prompt that would appear becomes an error exit instead of blocking on stdin; reduces spinner redraws. |
| `--accept-source-agreements` | Accepts source terms. Valid on install, upgrade, uninstall, list, show, export. Store passes it in no command. |
| `--accept-package-agreements` | Accepts package license text. Valid on install and upgrade only; uninstall exits 0x8A150002 (spike: uninstall takes only the source flag). Store passes it in no command. |
| `--scope user\|machine` | install: choose scope. uninstall/upgrade/list: filter on installed scope. `machine` needs elevation. |
| `--silent` (`-h`) | Silent installer. Pass for install/upgrade/uninstall (SPEC §6.2). Excludes `--interactive`. |
| `--exact` (`-e`) | Always pass with `--id` so `Foo.Bar` never matches `Foo.BarBaz`. |
| `--id`, `--source`, `--version` | Exact id; restrict source; pin version (`-v`). |
| `--verbose-logs` | Writes winget's own log file (path under `--info` "Logs"). No stdout change. Pass on install/upgrade (SPEC §6.2). |
| `--nowarn` | Suppresses warnings. Do not pass; warnings feed the log tail. |
| `--ignore-security-hash`, `--force` | NEVER. |
| `--include-unknown` | upgrade/list: include packages whose installed version is unknown (update checks, SPEC §6.3). |
| `--include-pinned` | Do not pass; pinned packages are skipped (Issue #7). |
| Encoding | Piped stdout is UTF-8 (`String::from_utf8_lossy`). Lines end `\r\n`. Output contains non-ASCII (bar glyphs, localized names). |
| Redraw noise | winget rewrites lines with `\r` for spinners (`- \ \| /`). Split on both `\r` and `\n`; drop single-char spinner lines. |

## Command lines

```text
install:   winget install --id ID --exact --source S [--version V] [--scope user|machine]
             --silent --disable-interactivity --verbose-logs
uninstall: winget uninstall --id ID --exact --source S --silent --disable-interactivity
update:    winget upgrade --id ID --exact --source S --silent --disable-interactivity --verbose-logs
installed: winget list --disable-interactivity
updates:   winget upgrade --include-unknown --disable-interactivity
           (equivalent: winget list --upgrade-available --include-unknown)
pin:       winget pin add --id ID --exact | winget pin remove --id ID --exact
```

No command line has an agreement flag.

## Output parsing

Line-oriented and tolerant: unknown lines go to the log tail and never change the stage.
Fraction is monotonic. Exit code decides success; stdout only drives progress.
The `Found`, `Downloading`, hash, `Starting package install/uninstall...` and `Successfully
installed/uninstalled` lines were OBSERVED in the Issue #4 smoke test (jq, winget 1.29.380).
The bar line, license notices, `Installer failed with exit code` and the agreement prompts are still INFERRED.

| Line pattern | Stage | Fraction |
|---|---|---|
| `Found <Name> [<Id>] Version <v>` | Resolving | 0.05 |
| `This application is licensed to you by its owner.` / `Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.` | stay Resolving; note agreements were shown | - |
| `Downloading <url>` | Downloading | 0.10 |
| Bar line: `██████▒▒▒▒  1.20 MB / 5.00 MB` (units B/KB/MB/GB) or `NN%` | Downloading | 0.10 + 0.50 * done/total |
| `Successfully verified installer hash` | Installing | 0.65 |
| `Starting package install...` / `Starting package uninstall...` | Installing | 0.70 |
| `Successfully installed` / `Successfully uninstalled` | Finalizing | 0.95 (Done only after exit 0) |
| `No installed package found matching input criteria.` / `No package found matching input criteria.` | Failed | `PackageNotFound` (exit 0x8A150014) |
| `No applicable upgrade found.` / `No available upgrade found.` | Done (no-op) | exit 0x8A15002B |
| `Installer failed with exit code: <n>` | Failed | `WingetFailed{code=n}` |
| `Do you agree to all the source agreements terms?` | blocked | exit 0x8A150046 |
| package-agreement prompt | blocked | exit 0x8A150041 |

Emit the terminal event exactly once, after the process exits: `Done` for 0, else `Failed` / `Cancelled`.
Bar glyphs: filled `█`, empty `▒`. With no byte counts, use filled cells over total cells.

### Table layout (OBSERVED)

`winget upgrade`, `winget list --upgrade-available`, `winget list` share one layout:

```text
Name        Id         Version   Available   Source     <- header
------------------------------------------------------   <- dashes, full width
<fixed-width rows>
6 upgrades available.                                    <- footer (upgrade / --upgrade-available only)
```

- Columns are space-padded. Take each column's start offset from the header (`Name`, `Id`, `Version`, `Available`, `Source`) and slice rows by character offset. Do NOT split on whitespace: names contain spaces.
- `Available` and `Source` can be blank (`winget list` rows with no catalog match). Such rows have ids like `ARP\Machine\X64\Bitdefender`; skip them for update logic.
- Skip everything before the first line that starts with `Name` (banner, source-update notices, progress).
- Footer `N upgrades available.` is a row-count sanity check (warn on mismatch, do not fail).
- No match: `No installed package found matching input criteria.`, exit 0x8A150014.
- Wide (CJK) names can misalign offsets; if the header lacks `Id`, return no rows plus a warn log.
- INFERRED: truncated cells may end with `…`, and a pin footer line can follow the count.

## Exit codes -> AppError

Full table with verified/inferred marks: `references/winget-cli.md`. Exit codes are u32 HRESULTs;
`ExitStatus::code()` yields `i32`, so compare `code as u32`.

| Exit (u32) | Symbol | Maps to |
|---|---|---|
| 0 | success | Done |
| 0x8A150014 | NO_APPLICATIONS_FOUND | `PackageNotFound(id)` |
| 0x8A150017 | NO_MANIFEST_FOUND (version not found) | not mapped yet; WP5 step 2 decides (spike Q2) |
| 0x8A150010 | NO_APPLICABLE_INSTALLER | `Unsupported("no installer for this system")` |
| 0x8A150011 | INSTALLER_HASH_MISMATCH | `WingetFailed` (security; never offer an override) |
| 0x8A15002B | UPDATE_NOT_APPLICABLE | Done, no-op |
| 0x8A150019 | COMMAND_REQUIRES_ADMIN | `ElevationRequired` |
| 0x8A150041 / 0x8A150046 | PACKAGE / SOURCE_AGREEMENTS_NOT_ACCEPTED | `AgreementsRequired` with `source_agreements`, `package_agreements`, `winget_source`, `version`, `agreement_text`, `agreement_url`. The terminal stage is `failed`, the `operation_log` row is Info, the ledger goes to the rest state (SPEC §6.7). `agreement_text` and `agreement_url` are null until F4 |
| 0x8A150005, 0x8A15006A, 0x8A15010C | CTRL_SIGNAL, APPTERMINATION, INSTALL_CANCELLED_BY_USER | `Cancelled` |
| 740 (Win32 installer code) | ERROR_ELEVATION_REQUIRED | `ElevationRequired` (INFERRED) |
| anything else | - | `WingetFailed{code, stderr}` |

Correction to the original brief: 0x8A150010 is NO_APPLICABLE_INSTALLER (not "no applications
found"), 0x8A150011 is the hash mismatch, 0x8A150027 is a LibYAML error (not elevation), and
0x8A150018..1C are extension/admin/security/store-policy codes. Trust the table in the reference.

## Agreements (spike of 2026-11-13)

Decision of 2026-11-13: WP5 builds case (c) of SPEC §6.6. The app passes no agreement flag in
any operation; case (b) (consent with the link) is later work, F4 in `docs/ISSUE-BACKLOG.md`.

Answers to the spike questions of SPEC §6.6. Evidence, commands and source lines:
`docs/spikes/2026-11-13-agreement-consent.md`. winget v1.29.380.

1. **Agreement text.** It comes from the agreement data of the package manifest
   (`Agreements: [{AgreementLabel, Agreement, AgreementUrl}]`) or of the REST source
   information (`SourceAgreements`). Each agreement is a label, an optional text and an optional
   URL. With `--disable-interactivity` and no flag, winget prints the block on stdout and then
   exits 0x8A150041 (package) or 0x8A150046 (source). `winget show --id <id> --version <v>
   --source <s> --disable-interactivity` prints the package block before any install.
   - Package block (install): starts with `Agreements for <name> [<id>] Version <v>`, then the
     show fields, then `Agreements:` and one `  <Label>: <text or URL>` line per agreement.
     Ends with `Package agreements were not agreed to. Operation cancelled.`
   - Source block: starts with ``The `<source>` source requires that you view the following
     agreements before using.``, then `<Label>: <text and/or URL>`. Ends with `One or more of the
     source agreements were not agreed to. ...`
   - OBSERVED: every agreement found (msstore source, msstore packages, `Microsoft.PowerBI` in
     the winget source) has a label and a URL and **no text**. Only REST sources (msstore) have a
     source agreement; the `winget` source has none (INFERRED from source).
   - `ProgressParser::agreements_shown()` matches the install disclaimer lines, which winget
     prints for every install. It does not detect an agreement.
2. **Missing `--version` on install:** 0x8A150017 NO_MANIFEST_FOUND, `No version found
   matching: <v>`. Not 0x8A150014. Observed on `show`; install uses the same function
   (source). Confirm at the first real-winget CI run (F2, #21).
3. **`winget uninstall` flags:** `--accept-source-agreements` only (VERIFIED in `--help` and by
   a run). `--accept-package-agreements` on uninstall exits 0x8A150002.
- winget stores a source acceptance per source name, keyed by `AgreementsIdentifier` and the
  implicit `market` field, and asks again only when one of them changes. It stores no package
  acceptance: each install of a package with agreements needs the flag again.

## Stage mapping

`OperationStage`: Queued, Resolving, Downloading, Installing, Finalizing, Done, Failed, Cancelled
(`is_terminal()` true for the last three). Emit `Queued` at op creation, `Resolving` after spawn.
`ProgressEvent::new` clamps fraction to 0..=1 and maps NaN to 0.
`InstallState` moves (domain/install.rs): NotInstalled -> Installing -> Installed | Failed | NotInstalled (cancel);
Installed -> Uninstalling -> NotInstalled | Failed. Use `InstallState::transition`; invalid moves return `Unsupported`.
The end of an operation follows the end table of SPEC §6.7. A cancel and an agreements stop
(`agreements_required`) put the ledger in the rest state (SPEC §0): `Installed` when a version
is recorded (`Pinned` when the pin flag is set), `NotInstalled` when no version is recorded. An
earlier `Failed` is not kept. A cancelled uninstall also goes to the rest state.

## Cancellation

winget has no cancel API. Cancel means: kill the process tree, then record Cancelled.

1. Spawn inside a Windows Job Object with `KILL_ON_JOB_CLOSE` (windows-rs `Win32_System_JobObjects`); fallback `taskkill /PID <pid> /T /F`.
2. `cancel(operation_id)` trips a cancel token; the read loop `select!`s token vs. child exit.
3. On cancel: terminate tree, `wait()` the child, emit one terminal `Cancelled` event, log it, return `AppError::Cancelled`.
4. If the child already exited 0 first, report Done.
5. Do not delete partial installer files; just report. Issue #6 puts `InstallState` in the rest state (SPEC §6.7); a cancelled uninstall too.
6. An elevated child cannot be killed from a non-elevated parent (access denied): log it, do not claim Cancelled.
7. The function that stops the tree is a field of the executor (`StopFn`, set with `WingetCli::with_stop_fn`; the default is the real `kill_tree`). When the tree is still alive after the stop, the executor keeps the Warn row and emits one `store:progress` event: stage unchanged, `cancel_failed = true`, message `cancel failed: the installer continues`. The send waits 2 s at most for room and never uses the slot of the terminal event; after 2 s it writes a Warn row. A second cancel does nothing. The UI part (disable Cancel) is WP5 step 3.

## Test strategy

- Fixture: `src-tauri/tests/fixtures/fake_winget.rs` (a dev-only cargo example, built by the tests; see its header for the script format).
- `STORE_WINGET_PATH` points the executor at the fake. Set it per test (temp dir), never process-global.
- Cover: each stage line, bar progress, table parse (reuse OBSERVED tables from the reference), every exit-code row above, cancel mid-download, cancel that cannot stop the tree (inject a stop function that reports a live tree with `WingetCli::with_stop_fn`; assert one `cancel_failed` event), no agreement flag in any argv (`install_passes_no_agreement_flag_in_any_case`, `no_command_passes_an_agreement_flag`), `--force` / `--ignore-security-hash` never in argv, `\r` spinner noise, invalid UTF-8 bytes, winget missing -> `WingetNotFound`.
- Real winget runs only in CI or Windows Sandbox (SPEC §8), never on a developer machine. Smoke (Issue #4 acceptance): install and remove one small package there; paste the real output into the reference. The smoke test passes no agreement flag.
- Gate: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt` (PATH note in CLAUDE.md).

## Install loop (Issue #6) glue

1. IPC `start_install`: create op id, take `OpGuard`, persist `InstallState::Installing`, emit Queued.
2. Relay each `ProgressEvent` to the frontend and the log tail; persist stage changes.
3. Exit 0 -> `Installed` and store the version (`winget list --id ID --exact`). Follow the end table of SPEC §6.7: a failure with a version before keeps `Installed`, a failed install with no version gives `Failed`, a cancel and an agreements stop go to the rest state.
4. Release the guard on every path (drop guard).
5. `is_paid` packages have no in-app install path; only the browser hand-off.
