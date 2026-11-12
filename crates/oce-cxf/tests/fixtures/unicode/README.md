# Unicode general categories `Cc` and `Cf`

`DerivedGeneralCategory-17.0.0-Cc-Cf.txt` is a verbatim excerpt of the Unicode Character Database
file `extracted/DerivedGeneralCategory.txt`, version 17.0.0. It is the independent source the unit
test `the_format_table_holds_every_code_point_unicode_17_assigns_to_cf_and_no_other` in
`crates/oce-cxf/src/format_controls.rs` reads: every Unicode scalar value is checked against it,
so the hand-written `Cf` table (`FORMAT_RANGES`) and `char::is_control` must each agree with it
code point by code point (issue #629; the second review of GitHub #641 found that a range shifted by
one passed the shape-only tests).

Nothing reads it at run time, and no test fetches anything.

## How it was cut

1. Download `https://www.unicode.org/Public/17.0.0/ucd/extracted/DerivedGeneralCategory.txt`
   (277,514 bytes; SHA-256
   `d62e5bab70ca74f099343f71224fa051cb1fdd61a1ab45c0488c44cfc0b6102e`; its first line is
   `# DerivedGeneralCategory-17.0.0.txt`, dated 2025-07-24).
2. Keep its header and its `General_Category=Control` and `General_Category=Format` sections,
   unchanged, with nothing added:

   ```bash
   sed -n '1,8p;3530,3563p' DerivedGeneralCategory.txt > DerivedGeneralCategory-17.0.0-Cc-Cf.txt
   ```

   The excerpt is 42 lines (SHA-256
   `3af2141d00bbef4e312424ac7042b6ed13e51744f050f68472ca81089b977d93`). Line 3530 is
   `# General_Category=Control` and line 3563 the Format section's `# Total code points: 170`.
3. Cross-check: Python 3.15.2's `unicodedata` (`unidata_version` `17.0.0`) assigns exactly the 235
   code points the excerpt lists to `Cc` (65) and `Cf` (170), and no others.

A later Unicode version is a new excerpt cut the same way, the table in `format_controls.rs`
updated to match, and this file's digests and line numbers with it.

## License

The excerpt is Unicode data, not open-control code, and it is not under the repository's MIT OR
Apache-2.0. Unicode's terms of use (<https://www.unicode.org/copyright.html>, which the data file's
header reaches through <https://www.unicode.org/terms_of_use.html>) put every data file under
`https://www.unicode.org/Public/` under the **Unicode License v3**. That license grants its
permissions provided its copyright and permission notice appears with every copy, so the notice
travels with the excerpt. [`LICENSE`](LICENSE) beside it is the license's full text, copied byte
for byte from <https://www.unicode.org/license.txt> on 2026-11-12 (1,995 bytes; SHA-256
`e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`; it reads "Copyright © 1991-2026
Unicode, Inc."). The excerpt also keeps the data file's own copyright line. Both files sit inside
`oce-cxf`'s package root, so a `cargo package` of the crate ships them together.
