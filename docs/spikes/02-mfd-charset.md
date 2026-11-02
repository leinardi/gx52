# S2: MFD character set (X52 Pro)

The MFD shows one byte per character from a fixed 256-entry table. Read from photos of all 256
values on the device (`x52-probe charset`, 2026-10-28) and cross-checked against the glyph list in
libx52's `x52_char_map.cfg` for the ambiguous ones (0x0E, 0x0F, 0x10, 0x9B to 0x9F). The table
below is our own transcription; libx52's file is GPL-2.0-only and is not copied.

Summary:

- `0x00`-`0x07` and `0xA0`: blank.
- `0x08`-`0x1F`: arrows, box corners, `·`, `®`, `©`, `™`, `†`, `§`, `¶` and Greek letters.
- `0x20`-`0x7F`: ASCII, except `0x5C` = `¥`, `0x7E` = `→`, `0x7F` = `←`.
- `0x80`-`0x9F`: Latin accented letters (`0x80`-`0x9A` as in code page 437), then `ñ Ñ ª º ¿`.
- `0xA1`-`0xDF`: JIS X 0201 half-width katakana (`U+FF61`-`U+FF9F`).
- `0xE0`-`0xFF`: more Latin letters, currency, fractions and math symbols.

The full table, and the rules the app follows when converting text (per-character conversion,
fallbacks, the 16-byte limit, the preview), are in
[protocol.md, MFD character set](../protocol.md#mfd-character-set). This spike keeps only how the
table was read.
