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
| Surface agreements, never pre-accept | Pass `--accept-source-agreements` / `--accept-package-agreements` ONLY when `AppConfig.accept_source_agreements` is true. Otherwise omit them; if winget exits with an agreements code, hand it to the UI as a prompt. |
| One op per package id | Acquire `OpGuard` (per-package mutex map, SPEC §1.2) before spawning. |
| Log every op | Write `operation_log` rows (start, each warn/error line, final exit status). Raw stderr goes in `detail`. |
| Never use `--force` or `--ignore-security-hash` | No code path, no config switch. |
| No real winget in unit tests or CI | Use the fake fixture. Real winget only in the manual smoke test. |

## Fixed facts

| Topic | Fact |
|---|---|
| Discovery order | 1) `STORE_WINGET_PATH` env var (tests/override), 2) `winget.exe` on PATH, 3) `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe`, 4) `Get-AppxPackage Microsoft.DesktopAppInstaller` then `InstallLocation\winget.exe`. None found -> `AppError::WingetNotFound`. |
| Version probe | `winget --version` prints `v1.29.380` (strip `v`). `winget --info` adds directories and admin settings. |
| Minimum version | Require >= 1.6.0 (project choice, not from docs: all flags below exist well before it). Lower -> `AppError::Unsupported("winget >= 1.6 required")`. Raise only on evidence. |
| Sources here | `msstore` (storeedgefd.dsx.mp.microsoft.com/v9.0), `winget` (cdn.winget.microsoft.com/cache), `winget-font` (explicit). Always pass `--source` explicitly. |
| `--disable-interactivity` | Always pass. A prompt that would appear becomes an error exit instead of blocking on stdin; reduces spinner redraws. |
| `--accept-source-agreements` | Accepts source terms. Valid on install, upgrade, uninstall, list, show, export. |
| `--accept-package-agreements` | Accepts package license text. Valid on install and upgrade only. |
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
             --silent --disable-interactivity --verbose-logs [accept flags]
uninstall: winget uninstall --id ID --exact [--source S] [--scope ..] --silent --disable-interactivity
update:    winget upgrade --id ID --exact --source S --silent --disable-interactivity --verbose-logs [accept flags]
installed: winget list --disable-interactivity [--source S]
updates:   winget upgrade --include-unknown --disable-interactivity
           (equivalent: winget list --upgrade-available --include-unknown)
pin:       winget pin add --id ID --exact | winget pin remove --id ID --exact | winget pin list
```

"accept flags" are added only when config `accept_source_agreements` is true.

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
| 0x8A150010 | NO_APPLICABLE_INSTALLER | `Unsupported("no installer for this system")` |
| 0x8A150011 | INSTALLER_HASH_MISMATCH | `WingetFailed` (security; never offer an override) |
| 0x8A15002B | UPDATE_NOT_APPLICABLE | Done, no-op |
| 0x8A150019 | COMMAND_REQUIRES_ADMIN | `ElevationRequired` |
| 0x8A150041 / 0x8A150046 | PACKAGE / SOURCE_AGREEMENTS_NOT_ACCEPTED | agreement prompt for the UI; until it exists, `AgreementsRequired` |
| 0x8A150005, 0x8A15006A, 0x8A15010C | CTRL_SIGNAL, APPTERMINATION, INSTALL_CANCELLED_BY_USER | `Cancelled` |
| 740 (Win32 installer code) | ERROR_ELEVATION_REQUIRED | `ElevationRequired` (INFERRED) |
| anything else | - | `WingetFailed{code, stderr}` |

Correction to the original brief: 0x8A150010 is NO_APPLICABLE_INSTALLER (not "no applications
found"), 0x8A150011 is the hash mismatch, 0x8A150027 is a LibYAML error (not elevation), and
0x8A150018..1C are extension/admin/security/store-policy codes. Trust the table in the reference.

## Stage mapping

`OperationStage`: Queued, Resolving, Downloading, Installing, Finalizing, Done, Failed, Cancelled
(`is_terminal()` true for the last three). Emit `Queued` at op creation, `Resolving` after spawn.
`ProgressEvent::new` clamps fraction to 0..=1 and maps NaN to 0.
`InstallState` moves (domain/install.rs): NotInstalled -> Installing -> Installed | Failed | NotInstalled (cancel);
Installed -> Uninstalling -> NotInstalled | Failed. Use `InstallState::transition`; invalid moves return `Unsupported`.

## Cancellation

winget has no cancel API. Cancel means: kill the process tree, then record Cancelled.

1. Spawn inside a Windows Job Object with `KILL_ON_JOB_CLOSE` (windows-rs `Win32_System_JobObjects`); fallback `taskkill /PID <pid> /T /F`.
2. `cancel(operation_id)` trips a cancel token; the read loop `select!`s token vs. child exit.
3. On cancel: terminate tree, `wait()` the child, emit one terminal `Cancelled` event, log it, return `AppError::Cancelled`.
4. If the child already exited 0 first, report Done.
5. Do not delete partial installer files; just report. Issue #6 resets `InstallState` to the prior state.
6. An elevated child cannot be killed from a non-elevated parent (access denied): log it, do not claim Cancelled.

## Test strategy

- Fixture: `src-tauri/tests/fixtures/fake_winget.rs` (a dev-only cargo example, built by the tests; see its header for the script format).
- `STORE_WINGET_PATH` points the executor at the fake. Set it per test (temp dir), never process-global.
- Cover: each stage line, bar progress, table parse (reuse OBSERVED tables from the reference), every exit-code row above, cancel mid-download, agreements flags absent unless config true (assert argv), `--force` / `--ignore-security-hash` never in argv, `\r` spinner noise, invalid UTF-8 bytes, winget missing -> `WingetNotFound`.
- CI never runs real winget. Manual smoke (Issue #4 acceptance): install and remove one small package on Windows; paste the real output into the reference.
- Gate: `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt` (PATH note in CLAUDE.md).

## Install loop (Issue #6) glue

1. IPC `start_install`: create op id, take `OpGuard`, persist `InstallState::Installing`, emit Queued.
2. Relay each `ProgressEvent` to the frontend and the log tail; persist stage changes.
3. Exit 0 -> `Installed` and store the version (`winget list --id ID --exact`). Error -> `Failed`. Cancel -> prior state.
4. Release the guard on every path (drop guard).
5. `is_paid` packages have no in-app install path; only the browser hand-off.
