# winget 1.29 fixtures

The `.txt` files are exact stdout of `winget search` / `winget show` and carry no header of
our own, so they stay plain copies of winget's output.

**Provenance: hand-made from the documented layout; re-record from real winget in plan step 6**
(`docs/designs/catalog-winget-cli.md`). They were not recorded from winget 1.29.380. The
SHA256 in `show-vlc.txt` is a placeholder. English Windows layout only.

- `search-vlc.txt` - `winget search --query vlc --source winget`: header, dash line, four rows, Match column.
- `search-none.txt` - no result (winget exits 0x8A150014).
- `search-same-name.txt` - two packages with one name, and the cut-list footer (`<...`).
- `show-vlc.txt` - `winget show --id VideoLAN.VLC --exact --source winget`.
- `show-multiline.txt` - a show where `Description:` and `Release Notes:` stand alone on their key line and the value follows, indented two spaces.
