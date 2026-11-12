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

## Terms of use

The excerpt keeps the file's copyright line. Unicode data files are distributed under the Unicode
License v3, which permits copying them with that notice; see
<https://www.unicode.org/terms_of_use.html> and <https://www.unicode.org/license.txt>.
