# Spike: agreement consent (SPEC §6.6, WP5 step 1)

Date: 2026-11-13. Issue #6, branch `impl/6--install-loop` (PR #19).
winget: v1.29.380 (`Microsoft.DesktopAppInstaller` v1.29.380.0), Windows.Desktop 10.0.22631.6199.
winget-cli source: `microsoft/winget-cli` at commit `57d424f360608411e871c44c7d0ba21d848bcab6` (master on 2026-11-13).

The answers for the skill are in `.claude/skills/winget-executor/SKILL.md`, section
"Agreements (spike of 2026-11-13)". This file holds the evidence.

## Result

| Question | Answer | Gates |
|---|---|---|
| 1. Where does the agreement text come from? | From the agreement data of the package manifest and of the source information. winget prints it on stdout with no prompt: `winget show` prints it, and an install prints it before it ends with 0x8A150041 or 0x8A150046. Each agreement is a label, an optional text and an optional URL. All three agreements that the spike found have a label and a URL, and **no text**. | Yes |
| 2. Exit code for an install with a `--version` that does not exist? | 0x8A150017 `APPINSTALLER_CLI_ERROR_NO_MANIFEST_FOUND` (-1978335209), message `No version found matching: <version>`. It is **not** `package_not_found` (0x8A150014). Source-derived for install; observed for `show`. | No |
| 3. Which agreement flags does `winget uninstall` accept? | `--accept-source-agreements` only. `--accept-package-agreements` fails with 0x8A150002. | Yes |

**Case:** not (a). winget gives agreement text only when the publisher puts text in the
manifest. The spike found no such agreement. The three agreements found carry a link only.
The maintainer chooses between (b) and (c).

**Question 3 selects the first branch of §6.6:** `start_uninstall` gets `options.consent`, and
`PackageExecutor::remove` gets a request. The consent of an uninstall can hold the source kind
only; the core must never pass `--accept-package-agreements` to `uninstall`.

## Where each command ran

All commands ran on the maintainer's developer machine and were read-only: they install,
change or remove nothing. No command ran `winget install`. The install behaviour comes from the
winget-cli source code. No step used Windows Sandbox or CI. The real-install check stays with
F2 (#21).

## Evidence

### E1. Sources and their agreements

```
$ winget source list
Name        Argument                                      Explicit
msstore     https://storeedgefd.dsx.mp.microsoft.com/v9.0 false
winget      https://cdn.winget.microsoft.com/cache        false
winget-font https://cdn.winget.microsoft.com/fonts        true
exit 0
```

The source agreement of `msstore` comes from the REST `information` endpoint of the source
(public GET, no account):

```
$ curl -s https://storeedgefd.dsx.mp.microsoft.com/v9.0/information
{... "SourceIdentifier":"StoreEdgeFD",
 "SourceAgreements":{"AgreementsIdentifier":"StoreEdgeFD",
   "Agreements":[{"AgreementLabel":"Terms of Transaction",
                  "AgreementUrl":"https://aka.ms/microsoft-store-terms-of-transaction"}]},
 "ServerSupportedVersions":["1.0.0","1.1.0","1.6.0"],
 "RequiredQueryParameters":["market"],"RequiredPackageMatchFields":["market"]}
```

The source agreement has a label and a URL, and no text. `market` makes winget add one more
line: "The source requires the current machine's 2-letter geographic region to be sent to the
backend service to function properly (ex. "US")." (`SourceAgreementsMarketMessage`).

Source code: `HandleSourceAgreementsForOneSource` in
`src/AppInstallerCLICore/Workflows/PromptFlow.cpp` (lines 43-106) prints the title
"The `{0}` source requires that you view the following agreements before using.", then for each
agreement the label, the text if not empty, and the URL if not empty. Then, if
`--accept-source-agreements` is absent and interactivity is off, it does not accept, and
`HandleSourceAgreements` (lines 434-457) prints "One or more of the source agreements were not
agreed to. ..." and ends with 0x8A150046. The text goes to stdout before the exit.

`SourceAgreements` is set only by REST sources (code search in
`src/AppInstallerRepositoryCore`: the `Rest/` files, `RepositorySource.cpp`, `SourceList.cpp`).
`Source::CheckSourceAgreements` (`RepositorySource.cpp` line 748) returns true when a source
has no agreements. Of the default sources, only `msstore` thus has a source agreement; the
pre-indexed `winget` source has none. *Inferred from source; not observed with a fresh
machine.*

### E2. Package agreements in `winget show`

msstore package (read-only, no flag, interactivity off; this machine accepted the msstore
source agreement before):

```
$ winget show --id 9NBLGGH4NNS1 --source msstore --disable-interactivity
Found App Installer [9NBLGGH4NNS1]
...
License: http://go.microsoft.com/fwlink/?LinkID=530144
Agreements:
  Category: System Components
  Pricing: Free
  Free Trial: No
  Terms of Transaction: https://aka.ms/microsoft-store-terms-of-transaction
  Seizure Warning: https://aka.ms/microsoft-store-seizure-warning
  Store License Terms: https://aka.ms/microsoft-store-license
Installer:
  Installer Type: msstore
...
exit 0
```

winget source: the spike ran `winget show --id <id> --exact --source winget
--disable-interactivity` for 25 well-known packages and counted lines that start with
`Agreements:`. One package had agreements:

```
Microsoft.PowerBI: 1
Agreements:
  Microsoft Software License Terms: https://powerbi.microsoft.com/desktop-eula/
```

No agreements: Oracle.JavaRuntimeEnvironment, Docker.DockerDesktop, Oracle.VirtualBox,
Microsoft.VisualStudio.2022.BuildTools, Microsoft.Teams, Zoom.Zoom, NVIDIA.GeForceExperience,
Adobe.Acrobat.Reader.64-bit, Microsoft.Office, Microsoft.DotNet.SDK.8, Oracle.JDK.23,
Oracle.JDK.21, Valve.Steam, EpicGames.EpicGamesLauncher, Ubisoft.Connect,
ElectronicArts.EADesktop, Microsoft.SQLServer.2022.Developer,
Intel.IntelDriverAndSupportAssistant, Logitech.GHUB, Spotify.Spotify, Discord.Discord,
Autodesk.DesktopApp, Corsair.iCUE.5, Mozilla.Firefox.

The manifest in `microsoft/winget-pkgs` matches the output; it has no `Agreement` text field
(`manifests/m/Microsoft/PowerBI/2.159.1231.0/Microsoft.PowerBI.locale.en-US.yaml`, lines 33-35):

```yaml
Agreements:
- AgreementLabel: Microsoft Software License Terms
  AgreementUrl: https://powerbi.microsoft.com/desktop-eula/
```

The manifest schema allows an `Agreement` text field, so a package with text can exist. A
GitHub code search for `AgreementLabel` in `microsoft/winget-pkgs` returned 0 results (the
search does not index that repository), so the spike could not count such packages.

Source code: `ShowAgreements` in `src/AppInstallerCLICore/Workflows/ShowFlow.cpp` (lines 28-52)
prints `Agreements:`, then for each agreement `  <Label>: `, the text if not empty, and the URL
if not empty. `winget show` calls it from `ShowPackageInfo` (line 134).

### E3. Package agreements during an install (source only, A8)

`PackageAgreementsPrompt` in `PromptFlow.cpp` (lines 136-223):

- `PackageNeedsPrompt` is true when the manifest of the selected version has agreements.
- `ShowPackageAgreements` always runs first. It prints `Agreements for <name> [<id>] Version
  <version>` (`ReportIdentityForAgreements`) and then `ShowAgreementsInfo` (ShowFlow.cpp
  lines 64-83): version, publisher, license, URLs, and the `Agreements:` block of E2.
- `EnsurePackageAgreementsAcceptance` returns if `--accept-package-agreements` is present.
  Otherwise, with interactivity off, it prints "Package agreements were not agreed to.
  Operation cancelled." and ends with 0x8A150041.

The executor passes `--disable-interactivity` on each command (`executor/winget_cli.rs`
lines 164, 180, 194, 202, 210). An install with no flag thus prints the agreement block on
stdout and then ends with 0x8A150041. The core can read the same data before the operation
with `winget show --id <id> --version <v> --source <s> --disable-interactivity`.

### E4. Version that does not exist

```
PS> winget show --id jqlang.jq --exact --source winget --version 0.0.0-spike
No version found matching: 0.0.0-spike
exit 0x8A150017 (-1978335209)
```

Source code: `SelectSinglePackageVersionForInstallOrUpgrade` in
`src/AppInstallerCLICore/Workflows/UpdateFlow.cpp` (lines 340-380) calls
`GetManifestFromPackage(/* considerPins */ true)` when `--version` is present. `show` calls
the same function (`ShowFlow.cpp` line 230). It reaches `GetManifestWithVersionFromPackage`
(`WorkflowBase.cpp` lines 1466-1553), which prints `No version found matching: {0}` and ends
with `APPINSTALLER_CLI_ERROR_NO_MANIFEST_FOUND`. An install thus gives the same code as the
observed `show`. A package ID that does not exist ends earlier, with 0x8A150014.

### E5. Uninstall flags

```
$ winget uninstall --help
...
  --accept-source-agreements  Accept all source agreements during source operations
...
exit 0
```

The help lists no `--accept-package-agreements`. `UninstallCommand.cpp` line 41 declares only
`AcceptSourceAgreements`; `InstallCommand.cpp` lines 51 and 56 declare both.

```
PS> winget uninstall --id Spike.DoesNotExist.Zz --exact --accept-package-agreements
Argument name was not recognized for the current command: '--accept-package-agreements'
exit 0x8A150002 (-1978335230)

PS> winget uninstall --id Spike.DoesNotExist.Zz --exact --accept-source-agreements --disable-interactivity
No installed package found matching input criteria.
exit 0x8A150014 (-1978335212)
```

The second command shows that winget takes the source flag on `uninstall`. The package ID does
not exist, so nothing was removed.

### E6. The two facts that do not gate the work

- **winget keeps a source acceptance.** `SaveAcceptedSourceAgreements` stores the
  `AgreementsIdentifier` and the implicit fields (`market`) for each source name
  (`SourceList.cpp` lines 731+). The proof on this machine: the msstore `show` of E2 passed
  with no flag and with interactivity off.
- **winget asks again only when the identifier or the fields change.**
  `SourceList::CheckSourceAgreements` (`SourceList.cpp` lines 712-729) compares only
  `AgreementsIdentifier` and the fields. A change of the agreement URL or text with the same
  identifier does not cause a new prompt.
- winget keeps **no** package acceptance. `EnsurePackageAgreementsAcceptance` reads only the
  flag or the prompt. Each install of a package with agreements needs the flag again.

The dialog thus opens for the source kind only on a machine that did not accept the msstore
agreement, and for the package kind on each install of such a package.

### E7. A finding for step 2

`ProgressParser::agreements_shown()` (`executor/winget_parse.rs` lines 198-201) sets its flag
on "This application is licensed to you by its owner." and "Microsoft is not responsible for,
...". Those lines are `InstallationDisclaimer1` and `InstallationDisclaimer2` in
`winget.resw`; winget prints them for each install, with or without an agreement. The flag
does not detect an agreement. The agreement block starts with `Agreements for ` (package) or
with "The `<source>` source requires that you view the following agreements before using."
(source). The spike changed no code.

## Choice for the maintainer: (b) or (c)

The spike found no agreement with text. The `agreement_text` path of case (a) stays useful
for a manifest that has text; the field is `null` in the other cases.

| Option | Core | UI | Cost | Effect for the user |
|---|---|---|---|---|
| (b) Link fallback | Builds `options.consent` for install and (source kind only) uninstall. Reads label and URL of each agreement from stdout or from `winget show`, fills `agreement_url` and also `agreement_text` when winget gives text. | Dialog names the package, version, source and kind, and shows each label with its link; the link opens in the browser. Tests tagged (a, b, c) and (a, b). | Parser for the agreement block, `consent` option on two commands, the dialog, and its tests (WP5 steps 2 and 3 as planned). | Packages with agreements (msstore apps, Microsoft.PowerBI) install after one "Accept". |
| (c) No consent | Builds no `consent` option; the app never passes a flag. `start_uninstall` also gets no `options`. | No dialog. Each `agreements_required` shows "The agreement text is not available. Store cannot install this package." Tests tagged (a, b, c) only. | Less work: no parser for the block, no dialog, fewer tests. | No package with an agreement installs. On a machine that never accepted the msstore source agreement, no msstore package installs or uninstalls through the app. |

SPEC §6.6 records a maintainer approval of (b) on 2026-11-12. The spike does not change that
record; it asks the maintainer to confirm it with the facts above.

## Notes for the SPEC

- §6.6 flow, step 4, expects `package_not_found` for a missing version. winget gives
  0x8A150017. The exit-code map in `executor/winget_parse.rs` has no row for it today.
- The alternative "Ask winget, before the operation starts, if an agreement is necessary" is
  possible for the package kind: `winget show --version <v>` prints the `Agreements:` block
  (E2). The source kind shows only when the source agreement is not accepted.
