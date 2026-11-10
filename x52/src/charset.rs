// Copyright 2026 Roberto Leinardi.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The MFD character set: the 256 glyphs of the display and the conversion of text to its bytes.
//!
//! The table is the transcription of docs/protocol.md ("MFD character set"). Text is converted
//! character by character; a character with no glyph falls back to its base letter (`ő` to `o`)
//! or to `?`. The MFD never receives raw UTF-8.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::canonical_combining_class;

use crate::command::MfdBytes;

/// The glyph of every byte, `None` for the bytes the MFD shows as blank (`0x00`–`0x07`, `0xA0`).
#[rustfmt::skip]
pub const TABLE: [Option<char>; 256] = [
    /* 0x00 */ None, None, None, None, None, None, None, None,
    /* 0x08 */ Some('←'), Some('┌'), Some('┐'), Some('└'), Some('┘'), Some('·'), Some('®'), Some('©'),
    /* 0x10 */ Some('™'), Some('†'), Some('§'), Some('¶'), Some('Γ'), Some('Δ'), Some('Θ'), Some('Λ'),
    /* 0x18 */ Some('Ξ'), Some('Π'), Some('Σ'), Some('Υ'), Some('Φ'), Some('Ψ'), Some('Ω'), Some('α'),
    /* 0x20 */ Some(' '), Some('!'), Some('"'), Some('#'), Some('$'), Some('%'), Some('&'), Some('\''),
    /* 0x28 */ Some('('), Some(')'), Some('*'), Some('+'), Some(','), Some('-'), Some('.'), Some('/'),
    /* 0x30 */ Some('0'), Some('1'), Some('2'), Some('3'), Some('4'), Some('5'), Some('6'), Some('7'),
    /* 0x38 */ Some('8'), Some('9'), Some(':'), Some(';'), Some('<'), Some('='), Some('>'), Some('?'),
    /* 0x40 */ Some('@'), Some('A'), Some('B'), Some('C'), Some('D'), Some('E'), Some('F'), Some('G'),
    /* 0x48 */ Some('H'), Some('I'), Some('J'), Some('K'), Some('L'), Some('M'), Some('N'), Some('O'),
    /* 0x50 */ Some('P'), Some('Q'), Some('R'), Some('S'), Some('T'), Some('U'), Some('V'), Some('W'),
    /* 0x58 */ Some('X'), Some('Y'), Some('Z'), Some('['), Some('¥'), Some(']'), Some('^'), Some('_'),
    /* 0x60 */ Some('`'), Some('a'), Some('b'), Some('c'), Some('d'), Some('e'), Some('f'), Some('g'),
    /* 0x68 */ Some('h'), Some('i'), Some('j'), Some('k'), Some('l'), Some('m'), Some('n'), Some('o'),
    /* 0x70 */ Some('p'), Some('q'), Some('r'), Some('s'), Some('t'), Some('u'), Some('v'), Some('w'),
    /* 0x78 */ Some('x'), Some('y'), Some('z'), Some('{'), Some('|'), Some('}'), Some('→'), Some('←'),
    /* 0x80 */ Some('Ç'), Some('ü'), Some('é'), Some('â'), Some('ä'), Some('à'), Some('å'), Some('ç'),
    /* 0x88 */ Some('ê'), Some('ë'), Some('è'), Some('ï'), Some('î'), Some('ì'), Some('Ä'), Some('Å'),
    /* 0x90 */ Some('É'), Some('æ'), Some('Æ'), Some('ô'), Some('ö'), Some('ò'), Some('û'), Some('ù'),
    /* 0x98 */ Some('ÿ'), Some('Ö'), Some('Ü'), Some('ñ'), Some('Ñ'), Some('ª'), Some('º'), Some('¿'),
    /* 0xA0 */ None, Some('｡'), Some('｢'), Some('｣'), Some('､'), Some('･'), Some('ｦ'), Some('ｧ'),
    /* 0xA8 */ Some('ｨ'), Some('ｩ'), Some('ｪ'), Some('ｫ'), Some('ｬ'), Some('ｭ'), Some('ｮ'), Some('ｯ'),
    /* 0xB0 */ Some('ｰ'), Some('ｱ'), Some('ｲ'), Some('ｳ'), Some('ｴ'), Some('ｵ'), Some('ｶ'), Some('ｷ'),
    /* 0xB8 */ Some('ｸ'), Some('ｹ'), Some('ｺ'), Some('ｻ'), Some('ｼ'), Some('ｽ'), Some('ｾ'), Some('ｿ'),
    /* 0xC0 */ Some('ﾀ'), Some('ﾁ'), Some('ﾂ'), Some('ﾃ'), Some('ﾄ'), Some('ﾅ'), Some('ﾆ'), Some('ﾇ'),
    /* 0xC8 */ Some('ﾈ'), Some('ﾉ'), Some('ﾊ'), Some('ﾋ'), Some('ﾌ'), Some('ﾍ'), Some('ﾎ'), Some('ﾏ'),
    /* 0xD0 */ Some('ﾐ'), Some('ﾑ'), Some('ﾒ'), Some('ﾓ'), Some('ﾔ'), Some('ﾕ'), Some('ﾖ'), Some('ﾗ'),
    /* 0xD8 */ Some('ﾘ'), Some('ﾙ'), Some('ﾚ'), Some('ﾛ'), Some('ﾜ'), Some('ﾝ'), Some('ﾞ'), Some('ﾟ'),
    /* 0xE0 */ Some('á'), Some('í'), Some('ó'), Some('ú'), Some('¢'), Some('£'), Some('¥'), Some('₧'),
    /* 0xE8 */ Some('ƒ'), Some('¡'), Some('Ã'), Some('ã'), Some('Õ'), Some('õ'), Some('Ø'), Some('ø'),
    /* 0xF0 */ Some('˙'), Some('¨'), Some('°'), Some('`'), Some('´'), Some('½'), Some('¼'), Some('×'),
    /* 0xF8 */ Some('÷'), Some('≤'), Some('≥'), Some('«'), Some('»'), Some('≠'), Some('√'), Some('¯'),
];

/// What a character with no glyph and no base letter becomes.
const FALLBACK: u8 = b'?';

/// Every glyph's byte. Built from [`TABLE`] in byte order, keeping the first byte of a glyph that
/// appears twice, so `¥` is `0x5C`, `←` is `0x08` and `` ` `` is `0x60`.
static BYTES: LazyLock<BTreeMap<char, u8>> = LazyLock::new(|| {
    let mut bytes = BTreeMap::new();
    for (byte, glyph) in (0..=u8::MAX).zip(TABLE) {
        if let Some(glyph) = glyph {
            bytes.entry(glyph).or_insert(byte);
        }
    }
    bytes
});

/// The byte of `glyph`, or `None` when the MFD has no such glyph. Never a blank byte.
#[must_use]
pub fn byte_for(glyph: char) -> Option<u8> {
    BYTES.get(&glyph).copied()
}

/// The MFD bytes of `text`.
///
/// The text is first composed (NFC), so a letter typed with a combining accent finds its glyph.
/// Then each character becomes its glyph's byte; a character without a glyph becomes its base
/// letter when that has one (`ő` to `o`), otherwise `?`. A combining mark left over after
/// composition belongs to the character before it and adds no byte. The result never contains a
/// blank byte: the only blank is the space, `0x20`.
#[must_use]
pub fn encode(text: &str) -> Vec<u8> {
    text.nfc()
        .filter(|character| canonical_combining_class(*character) == 0)
        .map(|character| {
            byte_for(character)
                .or_else(|| character.nfd().next().and_then(byte_for))
                .unwrap_or(FALLBACK)
        })
        .collect()
}

/// The text the MFD shows for `bytes`, a blank byte shown as a space: the preview of a line.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| TABLE[usize::from(*byte)].unwrap_or(' '))
        .collect()
}

/// The text of one MFD line, converted: at most 16 bytes of the character set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MfdText(MfdBytes);

impl MfdText {
    /// An empty line.
    pub const EMPTY: Self = Self(MfdBytes::EMPTY);

    /// `text` converted with [`encode`].
    ///
    /// # Errors
    ///
    /// [`TextTooLong`] when the converted text is longer than 16 bytes.
    pub fn new(text: &str) -> Result<Self, TextTooLong> {
        let bytes = encode(text);
        MfdBytes::new(&bytes)
            .map(Self)
            .map_err(|_| TextTooLong { len: bytes.len() })
    }

    /// `text` converted with [`encode`] and cut to the first 16 bytes: for text the app makes
    /// up (a profile name, a button name), which is shown cut rather than not at all.
    ///
    /// Every glyph is one byte, so the cut always falls on a character boundary of the converted
    /// text.
    #[must_use]
    pub fn fit(text: &str) -> Self {
        let bytes = encode(text);
        let cut = bytes.get(..MfdBytes::MAX_LEN).unwrap_or(&bytes);
        // `encode` never yields a blank byte and the cut fits a line, so the empty line is
        // never what comes out.
        MfdBytes::new(cut).map_or(Self::EMPTY, Self)
    }

    /// The converted bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// What the MFD shows.
    #[must_use]
    pub fn preview(&self) -> String {
        decode(self.as_bytes())
    }
}

/// Text that does not fit an MFD line after conversion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the text is {len} bytes after conversion, more than the 16 of an MFD line")]
pub struct TextTooLong {
    len: usize,
}

impl TextTooLong {
    /// The length of the converted text, in bytes.
    #[must_use]
    pub const fn converted_len(&self) -> usize {
        self.len
    }
}

#[cfg(test)]
mod tests;
