# winget CLI reference (trimmed)

Captured on winget v1.29.380, Windows 11 (10.0.22631), 2026-11-09. Only read-only commands were run
(`--info`, `--help`, `source list`, `list`, `show`). No install, uninstall, or upgrade was executed.

## Sources (`winget source list`)

```text
Name        Argument                                      Explicit
msstore     https://storeedgefd.dsx.mp.microsoft.com/v9.0 false
winget      https://cdn.winget.microsoft.com/cache        false
winget-font https://cdn.winget.microsoft.com/fonts        true
```

## `winget --info` (trimmed)

```text
Windows Package Manager v1.29.380
Package: Microsoft.DesktopAppInstaller v1.29.380.0
Logs   %LOCALAPPDATA%\Packages\Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\LocalState\DiagOutputDir
User Settings  ...\LocalState\settings.json
Portable Links Directory (User)  %LOCALAPPDATA%\Microsoft\WinGet\Links
Admin Setting   State   (LocalManifestFiles, InstallerHashOverride, ... all Disabled)
```

## Command flags (from `--help`)

Common to every command: `-?/--help`, `--wait`, `--logs/--open-logs`, `--verbose/--verbose-logs`,
`--nowarn/--ignore-warnings`, `--disable-interactivity`, `--proxy`, `--no-proxy`.

### install (alias `add`)

`[-q <query>] -m,--manifest | --id | --name | --moniker | -v,--version | -s,--source | --scope user|machine | -a,--architecture |
--installer-type | -e,--exact | -i,--interactive | -h,--silent | --locale | -o,--log | --custom | --override | -l,--location |
--ignore-security-hash (never) | --allow-reboot | --skip-dependencies | --dependencies-only | --dependency-source |
--accept-package-agreements | --accept-source-agreements | --no-upgrade | --uninstall-previous | --force (never) | -r,--rename`

Help text note: "By default, install command will check package installed status and try to perform an upgrade if
applicable. Override with --force to perform a direct install." Do not use `--force`; use `--no-upgrade` if the
caller must not upgrade an existing install.

### uninstall (aliases `remove`, `rm`)

`--id | --name | --moniker | --product-code | -v,--version | --all,--all-versions | -s,--source | -e,--exact |
--scope | -i | -h,--silent | --force (never) | --purge | --preserve | -o,--log | --accept-source-agreements`
(no `--accept-package-agreements` here).

### upgrade (alias `update`)

No arguments: lists packages with upgrades. Flags: install-like set plus `-r,--recurse,--all`,
`-u,--unknown,--include-unknown`, `--pinned,--include-pinned`, `--scope` (installed-scope filter),
`--accept-package-agreements`, `--accept-source-agreements`, `--uninstall-previous`, `--purge`.

### list (alias `ls`)

`-q | --id | --name | --moniker | -s,--source | --tag | --cmd | -n,--count (1..1000) | -e,--exact | --scope |
--accept-source-agreements | --upgrade-available | -u,--include-unknown (only with --upgrade-available) |
--pinned,--include-pinned (only with --upgrade-available) | --details | --sort | --asc | --desc`

### show (alias `view`)

`-q | -m | --id | --name | --moniker | -v | -s | -e | --scope | -a | --installer-type | --locale | --versions |
--accept-source-agreements`

### pin

Sub-commands: `add`, `remove`, `list`, `reset`. Help note: "A pinned package may still upgrade on its own and be
upgraded from outside the Windows Package Manager." Use `winget pin add --help` for `--blocking`, `--version`, `--installed`, `--force` (never).

### export

`winget export -o <file> [-s source] [--include-versions] [--accept-source-agreements]`.

### settings

Sub-commands: `export`, `set`, `reset`. Not used by the executor.

## Sample output (OBSERVED)

`winget show --id Mozilla.Firefox --source winget --disable-interactivity` (head; lines end `\r\n`):

```text
Found Mozilla Firefox (en-US) [Mozilla.Firefox]
Version: 159.0
Publisher: Mozilla
Publisher Url: https://www.mozilla.org/
Moniker: firefox
Description: Firefox Browser, also known as ...
Homepage: https://www.mozilla.org/firefox/
License: MPL-2.0
Release Notes:
  Version 159.0, first offered to Release channel users on ...
```

Note: `show` prints `Found <Name> [<Id>]` with no trailing `Version`. Header form differs by command
(`install` prints `Found <Name> [<Id>] Version <v>`, OBSERVED; `uninstall` prints `Found <Name> [<Id>]`), so match on the `Found ` prefix and the bracketed id.

`winget list --upgrade-available --disable-interactivity`:

```text
Name                                                               Id                              Version         Available      Source
----------------------------------------------------------------------------------------------------------------------------------------
7-Zip 26.05 (x64)                                                  7zip.7zip                       26.05           26.06          winget
Microsoft Visual C++ 2015-2022 Redistributable (x86) - 14.44.35211 Microsoft.VCRedist.2015+.x86    14.44.35211.0   14.51.36247.0  winget
Windows Terminal                                                   Microsoft.WindowsTerminal       1.24.12741.0    1.25.2733.0    winget
6 upgrades available.
```

Column widths adapt to the longest cell, so offsets must come from the header on every run.

`winget list --count 6` (no upgrade filter; blank `Available`/`Source` for unmatched apps):

```text
Name                       Id                                    Version     Available Source
---------------------------------------------------------------------------------------------
7-Zip 26.05 (x64)          7zip.7zip                             26.05       26.06     winget
Android Studio             Google.AndroidStudio                  2026.2.2.9            winget
Armoury Crate Service      ARP\Machine\X64\Armoury Crate Service 6.7.6.0
```

## Install/uninstall stdout (OBSERVED 2026-11-09, winget 1.29.380)

Smoke test `tests/winget_smoke.rs` with `jqlang.jq` 1.8.2, agreements accepted. The executor
printed one line per stage change; each line below is the winget stdout line that moved the stage.

`winget install --id jqlang.jq --exact --source winget --silent ...`:

```text
Found jq [jqlang.jq] Version 1.8.2
Downloading https://github.com/jqlang/jq/releases/download/jq-1.8.2/jq-windows-amd64.exe
Successfully verified installer hash
Starting package install...
Successfully installed
```

`winget uninstall --id jqlang.jq --exact --silent ...`:

```text
Found jq [jqlang.jq]
Starting package uninstall...
Successfully uninstalled
```

Both exited 0. Not observed in this run: the download bar line (the jq download is about 1 MB
and the smoke output shows stage changes only), the license notice lines (they do not change the
stage), and `Installer failed with exit code: <n>`. Those patterns stay INFERRED.

## Exit codes

Source: generated `returnCodes.md` in microsoft/winget-cli
(`doc/windows/package-manager/winget/returnCodes.md`, header date 05/13/2026, produced by `winget error --output`).
The Learn URL `learn.microsoft.com/en-us/windows/package-manager/winget/returnCodes` returned HTTP 404 when
fetched, so the upstream generated file was used instead (it is the page's source). Marks: VERIFIED = row present in
that table. INFERRED = not in the table; from general Windows or installer knowledge.

winget exit code is the HRESULT as u32. Rust on Windows reports it as i32 (decimal column).

| Hex | Decimal (i32) | Symbol | Mark | Meaning |
|---|---|---|---|---|
| 0x00000000 | 0 | (success) | VERIFIED (universal) | Success |
| 0x8A150001 | -1978335231 | APPINSTALLER_CLI_ERROR_INTERNAL_ERROR | VERIFIED | Internal Error |
| 0x8A150002 | -1978335230 | APPINSTALLER_CLI_ERROR_INVALID_CL_ARGUMENTS | VERIFIED | Invalid command line arguments |
| 0x8A150003 | -1978335229 | APPINSTALLER_CLI_ERROR_COMMAND_FAILED | VERIFIED | Executing command failed |
| 0x8A150005 | -1978335227 | APPINSTALLER_CLI_ERROR_CTRL_SIGNAL_RECEIVED | VERIFIED | Cancellation signal received |
| 0x8A150008 | -1978335224 | APPINSTALLER_CLI_ERROR_DOWNLOAD_FAILED | VERIFIED | Downloading installer failed |
| 0x8A150010 | -1978335216 | APPINSTALLER_CLI_ERROR_NO_APPLICABLE_INSTALLER | VERIFIED | None of the installers are applicable for the current system |
| 0x8A150011 | -1978335215 | APPINSTALLER_CLI_ERROR_INSTALLER_HASH_MISMATCH | VERIFIED | The installer file's hash does not match the manifest |
| 0x8A150014 | -1978335212 | APPINSTALLER_CLI_ERROR_NO_APPLICATIONS_FOUND | VERIFIED | No packages found |
| 0x8A150016 | -1978335210 | APPINSTALLER_CLI_ERROR_MULTIPLE_APPLICATIONS_FOUND | VERIFIED | Multiple packages found matching the criteria |
| 0x8A150019 | -1978335207 | APPINSTALLER_CLI_ERROR_COMMAND_REQUIRES_ADMIN | VERIFIED | Command requires administrator privileges to run |
| 0x8A15001B | -1978335205 | APPINSTALLER_CLI_ERROR_MSSTORE_BLOCKED_BY_POLICY | VERIFIED | The Microsoft Store client is blocked by policy |
| 0x8A15001C | -1978335204 | APPINSTALLER_CLI_ERROR_MSSTORE_APP_BLOCKED_BY_POLICY | VERIFIED | The Microsoft Store app is blocked by policy |
| 0x8A15002B | -1978335189 | APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE | VERIFIED | No applicable update found |
| 0x8A15002C | -1978335188 | APPINSTALLER_CLI_ERROR_UPDATE_ALL_HAS_FAILURE | VERIFIED | winget upgrade --all completed with failures |
| 0x8A15002D | -1978335187 | APPINSTALLER_CLI_ERROR_INSTALLER_SECURITY_CHECK_FAILED | VERIFIED | Installer failed security check |
| 0x8A15002E | -1978335186 | APPINSTALLER_CLI_ERROR_DOWNLOAD_SIZE_MISMATCH | VERIFIED | Download size does not match expected content length |
| 0x8A150030 | -1978335184 | APPINSTALLER_CLI_ERROR_EXEC_UNINSTALL_COMMAND_FAILED | VERIFIED | Running uninstall command failed |
| 0x8A150034 | -1978335180 | APPINSTALLER_CLI_ERROR_IMPORT_INSTALL_FAILED | VERIFIED | Failed to install one or more imported packages |
| 0x8A150041 | -1978335167 | APPINSTALLER_CLI_ERROR_PACKAGE_AGREEMENTS_NOT_ACCEPTED | VERIFIED | Package agreements were not agreed to |
| 0x8A150046 | -1978335162 | APPINSTALLER_CLI_ERROR_SOURCE_AGREEMENTS_NOT_ACCEPTED | VERIFIED | Source agreements were not agreed to |
| 0x8A150049 | -1978335159 | APPINSTALLER_CLI_ERROR_MSI_INSTALL_FAILED | VERIFIED | Running MSI install failed |
| 0x8A15004B | -1978335157 | APPINSTALLER_CLI_ERROR_FAILED_TO_OPEN_ALL_SOURCES | VERIFIED | Failed to open one or more sources |
| 0x8A150061 | -1978335135 | APPINSTALLER_CLI_ERROR_PACKAGE_ALREADY_INSTALLED | VERIFIED | Found at least one version of the package installed. |
| 0x8A150068 | -1978335128 | APPINSTALLER_CLI_ERROR_PACKAGE_IS_PINNED | VERIFIED | The package has a pin that prevents upgrade. |
| 0x8A15006A | -1978335126 | APPINSTALLER_CLI_ERROR_APPTERMINATION_RECEIVED | VERIFIED | Application shutdown signal received |
| 0x8A15006D | -1978335123 | APPINSTALLER_CLI_ERROR_SERVICE_UNAVAILABLE | VERIFIED | A required service is busy or unavailable. Try again later. |
| 0x8A15007D | -1978335107 | APPINSTALLER_CLI_ERROR_ADMIN_CONTEXT_ACTION_PROHIBITED | VERIFIED | The requested operation is not permitted from an administrator context on packages installed within the user scope. |
| 0x8A150086 | -1978335098 | APPINSTALLER_CLI_ERROR_INSTALLER_ZERO_BYTE_FILE | VERIFIED | Downloaded zero byte installer; ensure that your network connection is working properly. |
| 0x8A150101 | -1978334975 | APPINSTALLER_CLI_ERROR_INSTALL_PACKAGE_IN_USE | VERIFIED | Application is currently running. Exit the application then try again. |
| 0x8A150102 | -1978334974 | APPINSTALLER_CLI_ERROR_INSTALL_INSTALL_IN_PROGRESS | VERIFIED | Another installation is already in progress. Try again later. |
| 0x8A150103 | -1978334973 | APPINSTALLER_CLI_ERROR_INSTALL_FILE_IN_USE | VERIFIED | One or more file is being used. Exit the application then try again. |
| 0x8A150104 | -1978334972 | APPINSTALLER_CLI_ERROR_INSTALL_MISSING_DEPENDENCY | VERIFIED | This package has a dependency missing from your system. |
| 0x8A150105 | -1978334971 | APPINSTALLER_CLI_ERROR_INSTALL_DISK_FULL | VERIFIED | There's no more space on your PC. Make space, then try again. |
| 0x8A150107 | -1978334969 | APPINSTALLER_CLI_ERROR_INSTALL_NO_NETWORK | VERIFIED | This application requires internet connectivity. Connect to a network then try again. |
| 0x8A150109 | -1978334967 | APPINSTALLER_CLI_ERROR_INSTALL_REBOOT_REQUIRED_TO_FINISH | VERIFIED | Restart your PC to finish installation. |
| 0x8A15010A | -1978334966 | APPINSTALLER_CLI_ERROR_INSTALL_REBOOT_REQUIRED_FOR_INSTALL | VERIFIED | Installation failed. Restart your PC then try again. |
| 0x8A15010B | -1978334965 | APPINSTALLER_CLI_ERROR_INSTALL_REBOOT_INITIATED | VERIFIED | Your PC will restart to finish installation. |
| 0x8A15010C | -1978334964 | APPINSTALLER_CLI_ERROR_INSTALL_CANCELLED_BY_USER | VERIFIED | You cancelled the installation. |
| 0x8A15010D | -1978334963 | APPINSTALLER_CLI_ERROR_INSTALL_ALREADY_INSTALLED | VERIFIED | Another version of this application is already installed. |
| 0x8A15010E | -1978334962 | APPINSTALLER_CLI_ERROR_INSTALL_DOWNGRADE | VERIFIED | A higher version of this application is already installed. |
| 0x8A15010F | -1978334961 | APPINSTALLER_CLI_ERROR_INSTALL_BLOCKED_BY_POLICY | VERIFIED | Organization policies are preventing installation. Contact your admin. |
| 0x8A150110 | -1978334960 | APPINSTALLER_CLI_ERROR_INSTALL_DEPENDENCIES | VERIFIED | Failed to install package dependencies. |
| 0x8A150113 | -1978334957 | APPINSTALLER_CLI_ERROR_INSTALL_SYSTEM_NOT_SUPPORTED | VERIFIED | Package not supported by the system. |
| 0x8A150114 | -1978334956 | APPINSTALLER_CLI_ERROR_INSTALL_UPGRADE_NOT_SUPPORTED | VERIFIED | The installer does not support upgrading an existing package. |
| 0x8A150115 | -1978334955 | APPINSTALLER_CLI_ERROR_INSTALL_CUSTOM_ERROR | VERIFIED | Installation failed with a custom installer error. |
| - | 740 | ERROR_ELEVATION_REQUIRED (Win32) | INFERRED | Installer itself needs elevation; passed through |
| - | 1602 | ERROR_INSTALL_USER_EXIT (MSI) | INFERRED | User cancelled the MSI install |
| - | 1641 | ERROR_SUCCESS_REBOOT_INITIATED (MSI) | INFERRED | MSI succeeded, reboot started; treat as success plus reboot flag |
| - | 3010 | ERROR_SUCCESS_REBOOT_REQUIRED (MSI) | INFERRED | MSI succeeded, reboot needed; treat as success plus reboot flag |
| 1603 | 1603 | ERROR_INSTALL_FAILURE (MSI) | INFERRED | MSI fatal error; arrives as `Installer failed with exit code: 1603` |

Installer pass-through (INFERRED): when the installer itself fails, winget prints `Installer failed with exit code: <n>`
and exits with an install-family HRESULT (often 0x8A150115 INSTALL_CUSTOM_ERROR, or 0x8A150049 for MSI), not the raw
installer code. Parse `<n>` from stdout to fill `WingetFailed.code`/detail when present. Manifests can also declare
success codes (for example 3010), so winget may exit 0 for them. Not confirmed here; test during the smoke run.

Other families in the upstream file, not listed above: 0x8A150001-0x8A15008E (general, includes source, pin, portable,
font, store-catalog, authentication, repair rows), 0x8A150101-0x8A150115 (install), 0x8A150201-0x8A150207 (installed
status), 0x8A15C001-0x8A15C114 (configuration). Unlisted codes map to `WingetFailed{code, stderr}`.
Authentication codes (0x8A150073-0x8A150078) must never be handled in app code: no account sign-in (SPEC §0).
