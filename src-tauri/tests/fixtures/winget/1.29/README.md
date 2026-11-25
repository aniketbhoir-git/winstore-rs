# winget 1.29 fixtures

The `.txt` files are exact stdout of `winget search` / `winget show` and carry no header of
our own, so they stay plain copies of winget's output. They keep CRLF line ends; the
`.gitattributes` rule `-text` for this folder stops git from converting them. English
Windows layout only.

## Provenance

- `upgrade-no-source.txt` - hand-made, not recorded. The layout of `winget upgrade
  --include-unknown --source winget` in 1.29: header `Name Id Version Available`, no `Source`
  column (like the real `search --source winget` header, which has no Source column). CRLF,
  column widths 27/27/13. Replace it with a recorded file when a hosted-runner
  check exists.

Real files: winget v1.29.380, GitHub `windows-latest` runner, run 45487151178, 2026-11-25.
stdout was a pipe; the bytes are as recorded (CRLF, no spinner bytes appeared).

- `search-vlc.txt` - real. `winget search --query vlc --source winget --disable-interactivity`.
  Header, dash line, ten rows, Match column (`Moniker:`, `Tag:`, or empty).
- `search-none.txt` - real. `winget search --query zzzqqqxxnotapackage ...`: the one line
  `No package found matching input criteria.` (winget exit code 0x8A150014 = -1978335212).
- `show-vlc.txt` - real. `winget show --id VideoLAN.VLC --exact --source winget --disable-interactivity`.
  `Description:` stands alone on its key line and the text follows, indented two spaces.
- `search-same-name.txt` - synthetic, derived from the real 1.29.380 layout (column widths
  27/27/11, 77-char dash line, CRLF). Two packages with one name, then the cut-list footer
  (`<additional entries truncated due to result limit>`). The footer text is not recorded.
- `show-multiline.txt` - synthetic, derived from the real 1.29.380 layout (key order, CRLF,
  indented values). A `Release Notes:` block on its own key line. VLC has no Release Notes,
  so that key is not confirmed by a recording.

## Measured facts (same run)

- No cut columns in a pipe: no `…` cell, widest line 77 characters, header
  `Name Id Version Match`.
- `winget show` takes 457 ms cold (first call of the job) and 258 ms warm.
- Exit code 0 for a search with results, with no agreement flag.
- `install-piped.txt` - synthetic (PR #42), `winget install` stdout when piped (1.29.240 and newer): stage lines only. Hand-made from the documented stage lines, not captured; not part of run 45487151178.

- `export-small.json` - synthetic, hand-made in the layout of `winget export` (schema 2.0:
  `Sources`, each with `SourceDetails` and `Packages`; three ids, no `Version`). Replace it with
  a file recorded on a hosted runner (hardening design §4 and "Open questions"); the export tests
  only need the shape.

## Recorded with pinned tooling (PR ci/smoke-pin-upgrade)

Run 45582821030, GitHub `windows-latest` runner, winget v1.29.380 (module and winget pinned to
1.29.380), 2026-11-25. Read-only calls, no agreement flag; neither asked for an agreement.

- `upgrade-real.txt` - real, trimmed. `winget upgrade --source winget --disable-interactivity`
  (22 rows on the runner; 4 kept, footer rewritten to `4 upgrades available.`). Header
  `Name Id Version Available`: no Source column. Same layout as `upgrade-no-source.txt`; only the
  column widths differ (they follow the longest cell: 60/34/13 here). CRLF. Runner software
  only, no personal data.
- `export-real.json` - real, trimmed to 3 of 51 ids. `winget export -o <file> --source winget
  --disable-interactivity`. Same schema 2.0 shape as `export-small.json` (`Sources[].Packages`,
  `SourceDetails`, `WinGetVersion`, no `Version` per package). Difference: winget writes tab
  indent and `"key" : value` spacing. LF line ends. The non-winget packages produce only
  `Installed package is not available from any source: ...` lines on stderr (exit 0).
- `upgrade-no-source.txt` and `export-small.json` stay: the tests count their specific rows.
  `tests/real_output_fixtures.rs` runs the parsers on the real files.
