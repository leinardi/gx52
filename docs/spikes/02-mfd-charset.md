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

Consequences for the app: text is converted per character through a Unicode-to-byte table built
from this one. Characters with no glyph get a fallback (decomposition to the base letter where
possible, for example `ő` to `o`, otherwise `?`); never send raw UTF-8 bytes. The UI can preview
the result. The 16-character limit is per byte after conversion.

| | x0 | x1 | x2 | x3 | x4 | x5 | x6 | x7 | x8 | x9 | xA | xB | xC | xD | xE | xF |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **0x** |  |  |  |  |  |  |  |  | ← | ┌ | ┐ | └ | ┘ | · | ® | © |
| **1x** | ™ | † | § | ¶ | Γ | Δ | Θ | Λ | Ξ | Π | Σ | Υ | Φ | Ψ | Ω | α |
| **2x** | SP | ! | " | # | $ | % | & | ' | ( | ) | * | + | , | - | . | / |
| **3x** | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | : | ; | < | = | > | ? |
| **4x** | @ | A | B | C | D | E | F | G | H | I | J | K | L | M | N | O |
| **5x** | P | Q | R | S | T | U | V | W | X | Y | Z | [ | ¥ | ] | ^ | _ |
| **6x** | `` ` `` | a | b | c | d | e | f | g | h | i | j | k | l | m | n | o |
| **7x** | p | q | r | s | t | u | v | w | x | y | z | { | \| | } | → | ← |
| **8x** | Ç | ü | é | â | ä | à | å | ç | ê | ë | è | ï | î | ì | Ä | Å |
| **9x** | É | æ | Æ | ô | ö | ò | û | ù | ÿ | Ö | Ü | ñ | Ñ | ª | º | ¿ |
| **Ax** |  | ｡ | ｢ | ｣ | ､ | ･ | ｦ | ｧ | ｨ | ｩ | ｪ | ｫ | ｬ | ｭ | ｮ | ｯ |
| **Bx** | ｰ | ｱ | ｲ | ｳ | ｴ | ｵ | ｶ | ｷ | ｸ | ｹ | ｺ | ｻ | ｼ | ｽ | ｾ | ｿ |
| **Cx** | ﾀ | ﾁ | ﾂ | ﾃ | ﾄ | ﾅ | ﾆ | ﾇ | ﾈ | ﾉ | ﾊ | ﾋ | ﾌ | ﾍ | ﾎ | ﾏ |
| **Dx** | ﾐ | ﾑ | ﾒ | ﾓ | ﾔ | ﾕ | ﾖ | ﾗ | ﾘ | ﾙ | ﾚ | ﾛ | ﾜ | ﾝ | ﾞ | ﾟ |
| **Ex** | á | í | ó | ú | ¢ | £ | ¥ | ₧ | ƒ | ¡ | Ã | ã | Õ | õ | Ø | ø |
| **Fx** | ˙ | ¨ | ° | `` ` `` | ´ | ½ | ¼ | × | ÷ | ≤ | ≥ | « | » | ≠ | √ | ¯ |

Empty cells are blank on the display. `SP` is the space character.
