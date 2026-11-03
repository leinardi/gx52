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

//! The done-when of roadmap M1.3, then the fallbacks and the length limit.

use super::*;
use crate::command::{Command, MfdLine};

/// The bytes the MFD shows as blank.
fn is_blank(byte: u8) -> bool {
    matches!(byte, 0x00..=0x07 | 0xa0)
}

/// The bytes whose glyph is also at a lower byte, and that lower byte.
const DUPLICATES: [(u8, u8); 3] = [(0xe6, 0x5c), (0x7f, 0x08), (0xf3, 0x60)];

#[test]
fn blank_bytes_have_no_glyph_and_every_other_byte_has_one() {
    for (byte, glyph) in (0..=u8::MAX).zip(TABLE) {
        assert_eq!(glyph.is_none(), is_blank(byte), "{byte:#04x}");
    }
}

#[test]
fn every_non_blank_non_duplicate_byte_round_trips() {
    let duplicates: Vec<u8> = DUPLICATES.iter().map(|(upper, _)| *upper).collect();
    for byte in (0..=u8::MAX).filter(|byte| !is_blank(*byte) && !duplicates.contains(byte)) {
        let glyph = decode(&[byte]);
        assert_eq!(encode(&glyph), [byte], "{byte:#04x} {glyph:?}");
    }
}

#[test]
fn duplicates_convert_to_the_lower_byte() {
    for (upper, lower) in DUPLICATES {
        assert_eq!(decode(&[upper]), decode(&[lower]), "{upper:#04x}");
        assert_eq!(encode(&decode(&[upper])), [lower], "{upper:#04x}");
    }
    assert_eq!(byte_for('¥'), Some(0x5c));
    assert_eq!(byte_for('←'), Some(0x08));
    assert_eq!(byte_for('`'), Some(0x60));
}

#[test]
fn the_only_glyphs_that_appear_twice_are_the_three_duplicates() {
    let glyphs: Vec<char> = TABLE.iter().flatten().copied().collect();
    let mut twice: Vec<char> = glyphs
        .iter()
        .filter(|glyph| glyphs.iter().filter(|other| other == glyph).count() > 1)
        .copied()
        .collect();
    twice.sort_unstable();
    twice.dedup();
    assert_eq!(twice, ['`', '¥', '←']);
}

#[test]
fn no_character_produces_a_blank_byte() {
    // Every Unicode scalar value, alone and composed with what follows it: only the space is a
    // blank on the display.
    let mut line = String::new();
    for character in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
        for byte in encode(character.encode_utf8(&mut [0; 4])) {
            assert!(!is_blank(byte), "{character:?} gave {byte:#04x}");
        }
        line.push(character);
    }
    assert!(encode(&line).iter().all(|byte| !is_blank(*byte)));
}

#[test]
fn ascii_converts_to_itself_except_backslash_and_tilde() {
    let printable: String = (' '..='}').collect();
    let bytes: Vec<u8> = printable
        .bytes()
        .map(|byte| if byte == b'\\' { b'?' } else { byte })
        .collect();
    assert_eq!(encode(&printable), bytes);
    // The MFD has ¥ at 0x5C and → at 0x7E, so \ and ~ have no glyph.
    assert_eq!(encode("~"), b"?");
}

#[test]
fn a_letter_without_a_glyph_falls_back_to_its_base_letter() {
    assert_eq!(encode("ő"), b"o");
    assert_eq!(encode("Ő"), b"O");
    assert_eq!(encode("ž"), b"z");
    assert_eq!(encode("ş"), b"s");
    // A letter with a glyph keeps it.
    assert_eq!(encode("é"), [0x82]);
    // Typed with a combining accent, the letter still finds its glyph.
    assert_eq!(encode("e\u{301}"), [0x82]);
    // A combining accent the MFD cannot show is dropped with the letter's base kept.
    assert_eq!(encode("q\u{301}"), b"q");
}

#[test]
fn a_character_without_a_glyph_or_a_base_letter_becomes_a_question_mark() {
    // ł has no canonical decomposition.
    assert_eq!(encode("ł"), b"?");
    assert_eq!(encode("🚀"), b"?");
    assert_eq!(encode("\n\t\u{0}"), b"???");
    assert_eq!(encode("\u{7f}"), b"?");
    // The no-break space has only a compatibility decomposition, so it is not a space.
    assert_eq!(encode("\u{a0}"), b"?");
    // A full-width letter is not its half-width or ASCII form either.
    assert_eq!(encode("Ａ"), b"?");
}

#[test]
fn half_width_katakana_and_symbols_have_their_bytes() {
    assert_eq!(encode("ｱｲｳ"), [0xb1, 0xb2, 0xb3]);
    assert_eq!(encode("½°"), [0xf5, 0xf2]);
    assert_eq!(encode("Ω"), [0x1e]);
    // The ohm sign composes to the Greek capital omega.
    assert_eq!(encode("\u{2126}"), [0x1e]);
}

#[test]
fn the_length_limit_counts_converted_bytes() {
    assert!(
        MfdText::new(&"é".repeat(16)).is_ok(),
        "16 characters, 32 bytes of UTF-8, 16 bytes"
    );
    assert_eq!(
        MfdText::new(&"é".repeat(17)).map_err(|error| error.converted_len()),
        Err(17)
    );
    // A dropped combining mark does not count.
    assert!(MfdText::new(&"q\u{301}".repeat(16)).is_ok());
    assert_eq!(
        MfdText::new("ABCDEFGHIJKLMNOPQ")
            .err()
            .map(|error| error.to_string()),
        Some("the text is 17 bytes after conversion, more than the 16 of an MFD line".to_owned())
    );
}

#[test]
fn the_preview_is_what_the_mfd_shows() {
    let text = MfdText::new("Ünïcödé ~ ő").unwrap();
    assert_eq!(text.preview(), "Ünïcödé ? o");
    assert_eq!(decode(&[0x00, 0x5c, 0xa0]), " ¥ ");
}

#[test]
fn an_odd_length_line_is_padded_with_a_space() {
    let text = MfdText::new("é").unwrap();
    let transfers: Vec<(u16, u16)> = Command::MfdText(MfdLine::One, text)
        .encode()
        .iter()
        .map(|transfer| (transfer.index(), transfer.value()))
        .collect();
    assert_eq!(transfers, [(0xd9, 0), (0xd1, 0x2082)]);
}
