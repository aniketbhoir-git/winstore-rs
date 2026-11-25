# winget 1.29 fixtures

The `.txt` files are exact stdout of `winget search` / `winget show` and carry no header of
our own, so they stay plain copies of winget's output. They keep CRLF line ends; the
`.gitattributes` rule `-text` for this folder stops git from converting them. English
Windows layout only.

## Provenance

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
