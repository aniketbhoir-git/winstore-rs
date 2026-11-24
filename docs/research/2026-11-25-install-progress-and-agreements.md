# Install progress and agreements: winget CLI, winget COM API, UniGetUI

Research date: 2026-11-25. Code links point to `master` (winget-cli) and `main` (UniGetUI).

## 1. How winget reports progress

### CLI (stdout)

- The CLI draws an indefinite spinner and a progress bar. The bar is 30 cells wide and handles two kinds of progress: `Bytes` and `Percent`.
  Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/ExecutionProgress.cpp (`s_ProgressBarCellWidth = 30`) and https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCommonCore/Public/AppInstallerProgress.h (`enum class ProgressType { None, Bytes, Percent }`).
- The bar uses VT (terminal escape) codes when the console supports them. All progress styles except `disabled` also send "Virtual Terminal progress" to the terminal (the taskbar progress).
  Source: https://github.com/microsoft/winget-cli/blob/master/doc/Settings.md (section `progressBar`).
- `--no-progress` turns progress off for one command. Source: same file.
- **Important change for this app.** Since `v1.29.160-preview` (stable `v1.29.240`, 2026-05-29), the CLI creates the spinner and bar only when a console is attached. When stdout goes to a pipe, there is no progress output at all.
  Sources: release notes "Spinner and progress bar output are suppressed when no console is attached, keeping redirected output clean." in https://github.com/microsoft/winget-cli/releases/tag/v1.29.240 and https://github.com/microsoft/winget-cli/releases/tag/v1.29.160-preview ; code in `Reporter::Reporter` at https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/ExecutionReporter.cpp (`if (GetConsoleWidth().has_value())`).
  I checked that tags up to `v1.28.240` do not have this check.
- ADR 0001 measured winget 1.29.380. So with a piped process, this app cannot read download percent from winget stdout on that version. On winget 1.28 and older it can (as text with `\r` redraws), but the format is not a contract.
- The package's own installer can show its own UI or print its own text. `--silent` hides it; `--interactive` shows it. Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/install

Ways to get progress with the CLI:

1. Run winget under a pseudo console (ConPTY) so `GetConsoleWidth()` succeeds, then parse the redraws. Fragile: the text can change in any release.
2. Show an indefinite spinner and the final exit code only. Simple and always works.
3. Use the COM API (below).

### COM API (`Microsoft.Management.Deployment`)

- Install returns `IAsyncOperationWithProgress<InstallResult, InstallProgress>`. The caller subscribes to the `Progress` event.
  Source: https://github.com/microsoft/winget-cli/blob/master/doc/specs/%23888%20-%20Com%20Api.md (section 3, examples) and https://github.com/microsoft/winget-cli/blob/master/src/Microsoft.Management.Deployment/PackageManager.idl
- `InstallProgress` has: `State` (`Queued, Downloading, Installing, PostInstall, Finished`), `BytesDownloaded`, `BytesRequired`, `DownloadProgress` (a double), `InstallationProgress` (a double, "if known").
  The IDL notes that bytes may be known only for downloads done by winget itself, and that a percentage for the whole install is "purposefully not included" because no one can estimate an installer's progress.
  Source: PackageManager.idl (link above).
- Cancel: cancelling the async operation during `Queued` or `Downloading` stops the install. During `Installing` or `PostInstall` it does not stop the installer or roll back.
  Source: PackageManager.idl, comments on `PackageInstallProgressState`.
- Version: `InstallProgress` is in contract version 1. NuGet `Microsoft.WindowsPackageManager.ComInterop` has versions from `1.4.10052`.
  Sources: PackageManager.idl; https://api.nuget.org/v3-flatcontainer/microsoft.windowspackagemanager.cominterop/index.json
- The spec says the API is for "packaged apps with the packageManagement capability and other higher privilege processes". Source: the COM spec above (section 2).
  UniGetUI is distributed unpackaged and still activates the COM server with `CLSCTX_LOCAL_SERVER`, using code with Microsoft copyright headers.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/WindowsPackageManager.Interop/WindowsPackageManager/WindowsPackageManagerStandardFactory.cs
  I did not run it from a Tauri app. This needs a test on a clean machine (see "Features").
- Rust: the `windows` crate does not ship bindings for this namespace. `windows-bindgen` can generate bindings from a `.winmd` file. The `.winmd` file comes with the ComInterop NuGet package.
  Source: https://github.com/microsoft/windows-rs/blob/master/crates/libs/bindgen/readme.md (custom metadata is supported). I did not build a prototype.

## 2. How UniGetUI shows progress

- UniGetUI starts `winget` as a child process for install, update and uninstall. It parses text output and keeps progress lines apart from the log.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/Helpers/WinGetPkgOperationHelper.cs
- Its operation layer has a manager-neutral `OperationProgress` record: stage (`Downloading, Installing, Updating, Uninstalling`), `Percentage`, `BytesDownloaded`, `BytesTotal`, `BytesPerSecond`.
  A null percentage means "unknown", and the UI must show an indeterminate bar. Download and install are never merged into one fake total.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Enums/OperationProgress.cs
- Speed is `deltaBytes / deltaTime`, smoothed (alpha 0.3). The speed text disappears after 2 s without new bytes. No ETA is computed.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Operations/AbstractOperation_Progress.cs (header comment)
- The live log folds carriage-return redraws (spinners, download bars) onto one line instead of stacking them.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Operations/ProgressLineCollapser.cs
- I found no code that takes progress events from the winget COM install call. In UniGetUI the COM path is used for search, listing and details. This is a negative finding from reading `NativeWinGetHelper.cs`.

Lesson: UniGetUI treats unknown progress as indeterminate, shows the log on demand, and never invents a total.

## 3. Agreements in winget

There are two kinds, and they use two flags.

### Source agreements

- Some sources ask the user to accept terms before use. winget prints the title, label, text and URL for each agreement, and a note when the source receives the user's market (country).
  Source: `HandleSourceAgreementsForOneSource` in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/Workflows/PromptFlow.cpp
- `--accept-source-agreements` accepts without a prompt. Without it, winget asks, but only if interactivity is allowed. Otherwise the answer is "not accepted".
  Source: same function; docs: https://learn.microsoft.com/en-us/windows/package-manager/winget/install
- The acceptance is saved per source and agreement identifier (`SaveAcceptedSourceAgreements`). It is asked again if the agreement text changes.
  Source: `SourceList.cpp` (`AcceptedAgreementsIdentifier`) at https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerRepositoryCore/SourceList.cpp
- Example: the Store source (`msstore`) lists "Terms of Transaction" at https://aka.ms/microsoft-store-terms-of-transaction, and needs the market. I read this from `https://storeedgefd.dsx.mp.microsoft.com/v9.0/information` on 2026-11-25.
- Exit code when not accepted: `0x8A150046` (`SOURCE_AGREEMENTS_NOT_ACCEPTED`). Source: https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerSharedLib/Public/AppInstallerErrors.h

### Package agreements

- A manifest can carry agreements (label, text, URL). winget prints them before the install, then asks for acceptance.
  Source: `PackageAgreementsPrompt` in PromptFlow.cpp (link above).
- `--accept-package-agreements` accepts them without a prompt. Learn says it "applies to the package's own license terms only".
  Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/install
- If the flag is absent and interactivity is off, winget fails with `0x8A150041` (`PACKAGE_AGREEMENTS_NOT_ACCEPTED`). Source: PromptFlow.cpp `EnsurePackageAgreementsAcceptance` and AppInstallerErrors.h.
- Interactivity is off when any one of these is true: the context is non-interactive, `--disable-interactivity` is set, or the user setting `interactivity.disable` is on.
  Source: `IsInteractivityAllowed` in https://github.com/microsoft/winget-cli/blob/master/src/AppInstallerCLICore/Workflows/PromptFlow.cpp ; the setting is described in https://github.com/microsoft/winget-cli/blob/master/doc/Settings.md (section "Interactivity")
- Learn page for the install command, "License Agreements": "From the command line, you can auto accept the agreements by passing ... `--accept-package-agreements`". Source: https://learn.microsoft.com/en-us/windows/package-manager/winget/install
- Some installers have their own EULA inside the installer. The flag does not accept those. Source: Learn page above ("does not affect optional components or bundled software offered by the installer").

### COM API

- `PackageAgreement` and `SourceAgreement` are classes with `Label`, `Text`, `Url`. `CatalogPackageMetadata.Agreements` lists the package agreements. Both are contract version 6.
  Source: PackageManager.idl (link above).
- `InstallOptions.AcceptPackageAgreements` and `PackageCatalogReference.AcceptSourceAgreements` are the flags. Without acceptance, the result status is `PackageAgreementsNotAccepted` (contract 6) or `SourceAgreementsNotAccepted`.
  Source: PackageManager.idl.
- Contract 6 first appears in tag `v1.5.1572` (contract 5 in `v1.4.10173`, contract 7 in `v1.6.2631`).
  Source: `[contractversion(N)]` line of PackageManager.idl at each tag, e.g. https://github.com/microsoft/winget-cli/blob/v1.5.1572/src/Microsoft.Management.Deployment/PackageManager.idl
- New in the current IDL (contract 31): `ImplicitAgreementFields` (the source receives the user's market) and `SourceAgreementsSatisfied`. A caller that shows agreements should also disclose these.
  Source: PackageManager.idl.

## 4. What UniGetUI shows the user

- UniGetUI accepts for the user. The CLI arguments for install and update always include `--accept-package-agreements --force`, and for every winget call `--accept-source-agreements --disable-interactivity`. Uninstall omits the package flags.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/Helpers/WinGetPkgOperationHelper.cs
- On the COM path it sets `AcceptSourceAgreements = true` on every catalog reference.
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/ClientHelpers/NativeWinGetHelper.cs
- The listing calls also pass `--accept-source-agreements` (`list`, `update`). Source: https://github.com/marticliment/UniGetUI/blob/main/src/UniGetUI.PackageEngine.Managers.WinGet/ClientHelpers/WinGetCliHelper.cs
- I found no prompt that shows package agreement text. The details page shows "License: {0}" with a link ("Show license" / "Hide license" strings).
  Source: https://github.com/marticliment/UniGetUI/blob/main/src/Languages/lang_en.json
  This is a negative finding (I read the winget helpers and the English string file, not every UI file).
- Consequence: UniGetUI users never see agreement text before an install. This is a legal and trust gap that this app can close.

## Features worth building

- **Install progress panel with honest states.**
  Show Queued, Downloading (bytes and percent if known), Installing, Finishing. Use an indeterminate bar when the percent is unknown, like UniGetUI. Never invent a total.
- **Progress through the COM API (spike).**
  Prove in a small test that an unpackaged Tauri process can activate the winget COM server and receive `InstallProgress` events. If it works, use it for install and upgrade; keep the CLI as a fallback. The CLI gives no progress when piped on winget 1.29.160 and later.
- **Agreement screen before install.**
  Read package and source agreements (COM `Agreements`, or `winget show`), show label, text and link, and ask once. Then pass `--accept-package-agreements`. This is better than UniGetUI's silent accept.
  This matches only the planned F4 case (b). The current SPEC §6.6 case (c) passes no agreement flag.
- **Source terms for the Store source.**
  If `msstore` is added, show its "Terms of Transaction" link and the note that the market (country) is sent. Save the acceptance with the agreement identifier.
- **Clear errors for agreement exit codes.**
  Map `0x8A150041` and `0x8A150046` to plain messages: "You must accept the terms to install this app." with a retry button.
- **Cancel button with a safe state rule.**
  Allow cancel while queued or downloading. After the installer starts, say that cancel cannot stop it (the COM API gives the same rule).
- **Fold redraw lines in the log view.**
  If the app shows winget output, fold `\r` redraws onto one line, like UniGetUI's `ProgressLineCollapser`.
