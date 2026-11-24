# Minimum Windows and winget versions

Research date: 2026-11-25. I found the first version of each feature by reading the winget-cli source at release tags (the file exists with the feature, or it does not). Release dates come from the GitHub releases list.
Source for the releases list: https://github.com/microsoft/winget-cli/releases

## Windows

- winget is supported on Windows 10 version 1809 (build 17763) or later, Windows 11, and Windows Server 2025.
  "The WinGet command line tool is only supported on Windows 10 version 1809 (build 17763) or later."
  Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/
- Older Windows 10 builds are dropped. Release note of `v0.2.2941-preview`: "We have discontinued support for versions of Windows 10 prior to version 1809."
  Source: https://github.com/microsoft/winget-cli/releases/tag/v0.2.2941-preview
- winget ships in the **App Installer**, which is a system component updated by the Microsoft Store (and by Windows Update on Server 2025). A new user may not have winget until the first sign-in finishes. Registration command: `Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe`.
  Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/
- The COM API has no separate Windows requirement in its spec. It follows the winget version. Source: https://github.com/microsoft/winget-cli/blob/master/doc/specs/%23888%20-%20Com%20Api.md

## winget version per feature

winget versions are `major.minor.build`. There is no `x.y.0` tag. The first stable 1.6 build is `v1.6.2482` (2023-09-06). ADR 0001 states the project minimum as "1.6.0". Read that as "any 1.6 build or newer".

| Feature | First version that has it | Evidence | Source |
|---|---|---|---|
| `--accept-source-agreements`, `--accept-package-agreements` | `v1.0.12576` (2021-09-17) | Flag names are in `Argument.cpp` at this tag, and not at `v1.0.11692`. | https://github.com/microsoft/winget-cli/blob/v1.0.12576/src/AppInstallerCLICore/Argument.cpp |
| `--disable-interactivity` | `v1.4.3132-preview` (2022-11-15); first stable tag `v1.4.3531` (2022-12-21) | Present in `Argument.cpp` at `v1.4.3132-preview`, absent at `v1.4.2161-preview`. | https://github.com/microsoft/winget-cli/blob/v1.4.3132-preview/src/AppInstallerCLICore/Argument.cpp |
| COM API (`Microsoft.Management.Deployment`), install with progress (contract 1) | In winget since the 1.2 to 1.3 era; the supported NuGet interop package starts at `1.4.10052` (2023-01-06) | Release notes mention the COM server and COM uninstall. NuGet index lists `1.4.10052` as the first stable version. | https://github.com/microsoft/winget-cli/releases/tag/v1.3.1251-preview ; https://api.nuget.org/v3-flatcontainer/microsoft.windowspackagemanager.cominterop/index.json |
| COM package and source agreements (`PackageAgreement`, `PackageAgreementsNotAccepted`; contract 6) | `v1.5.1572` (2023-06-07) | `[contractversion(5)]` at `v1.4.10173`, `(6)` at `v1.5.1572`, `(7)` at `v1.6.2631`. Agreement types are marked contract 6. | https://github.com/microsoft/winget-cli/blob/v1.5.1572/src/Microsoft.Management.Deployment/PackageManager.idl |
| `winget list --upgrade-available` | `v1.5.1572` (2023-06-07) | `ListCommand.cpp` has the upgrade argument at `v1.5.1572`, not at `v1.5.1361-preview` or earlier. Learn documents it. | https://github.com/microsoft/winget-cli/blob/v1.5.1572/src/AppInstallerCLICore/Commands/ListCommand.cpp ; https://learn.microsoft.com/en-us/windows/package-manager/winget/list |
| `--include-unknown` together with `list --upgrade-available` | `v1.6.2291-preview` (2023-08-18); stable `v1.6.2482` | Release note "Allow `--include-unknown` in list `--upgrade-available`" (PR 3473). | https://github.com/microsoft/winget-cli/releases/tag/v1.6.2482 |
| Progress bar drawn to piped stdout | Up to `v1.28.x` | Progress was created without a console check. The check `GetConsoleWidth().has_value()` is missing at `v1.28.240`. | https://github.com/microsoft/winget-cli/blob/v1.28.240/src/AppInstallerCLICore/ExecutionReporter.cpp |
| **No progress on piped stdout** (breaking for this app) | `v1.29.160-preview` (2026-05-01); stable `v1.29.240` (2026-05-29) | Release note: "Spinner and progress bar output are suppressed when no console is attached". | https://github.com/microsoft/winget-cli/releases/tag/v1.29.240 |
| `--no-progress` flag | `v1.29.70-preview` (2026-04-08); stable `v1.29.240` | Release note: "Added a new `--no-progress` command-line flag". Absent in `v1.28.220`, present in `v1.29.240`. | https://github.com/microsoft/winget-cli/releases/tag/v1.29.70-preview |

Notes:

- Tags with `-preview` are preview builds. Users on the Store channel get stable builds. Use the stable column for a user minimum.
- `winget list --upgrade-available` dates from the first source check at `v1.5.1572`. I did not test the tags between `v1.5.1361-preview` and `v1.5.1572`, because those tags did not show it. The first stable 1.5 build is the safe bound.
- The latest stable on 2026-11-25 is `v1.29.380` (2026-09-21), which is the version in ADR 0001. Latest preview is in the `v1.30` line.
  Source: https://github.com/microsoft/winget-cli/releases

## What this means for the app

1. **Baseline 1.6 (stable `v1.6.2482` or newer) covers every CLI feature in scope:** agreement flags, `--disable-interactivity`, `list --upgrade-available`, `--include-unknown`. It also covers COM agreements (contract 6, from 1.5.1572).
2. **Progress needs a different baseline.** On winget 1.29.160+ a piped child process gets no progress text. On 1.28 and older it does. The app must detect the winget version at start (`winget --version`) and choose a method: COM events, a pseudo console, or an indeterminate bar.
3. **Do not pass `--no-progress`** unless the version is 1.29.240 or newer. Older builds reject unknown flags. (I did not test the exact error.)
4. **Detect, do not assume.** Check the Windows build (17763 or newer) and the winget version at start. If winget is missing, show the install link for App Installer: https://apps.microsoft.com/detail/9nblggh4nns1 (from the Learn page above).

## Features worth building

- **Startup environment check.**
  Read the Windows build and `winget --version`. Show "Windows build too old", "winget missing" or "winget too old" with one fix button. Fresh sign-ins may have no winget yet (the registration command above).
- **Version-gated capability table in code.**
  Keep one table: feature, minimum winget, fallback. The UI reads it to enable COM progress, `--include-unknown` and `--no-progress`. This stops flag errors on old builds.
- **Progress method chooser.**
  Pick COM events when available, else an indeterminate bar. Never rely on parsing piped stdout on 1.29.160 or later.
- **Installed-updates view from `winget list --upgrade-available`.**
  Show apps with updates, with an option to include unknown versions (`--include-unknown`, from 1.6.2482 stable).
- **Compatibility test matrix in CI notes.**
  Record results for one 1.6, one 1.28 and one 1.29 winget build with the fake winget fixture and, when approved, the real smoke test. This catches output changes between versions.
