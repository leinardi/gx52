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

use super::*;

fn chars(count: usize) -> String {
    "x".repeat(count)
}

#[test]
fn names_are_trimmed_and_limited_to_64_characters() {
    assert_eq!(normalise("  Night flying "), Ok("Night flying".to_owned()));
    assert_eq!(normalise(""), Err(NameProblem::Empty));
    assert_eq!(normalise(" \t "), Err(NameProblem::Empty));
    assert_eq!(normalise(&chars(64)), Ok(chars(64)));
    assert_eq!(normalise(&chars(65)), Err(NameProblem::TooLong(65)));
    // Characters, not bytes: 64 two-byte characters are allowed.
    assert_eq!(normalise(&"é".repeat(64)), Ok("é".repeat(64)));
}

#[test]
fn truncate_cuts_long_names_and_rejects_empty_ones() {
    assert_eq!(truncate(&chars(80)), Some(chars(64)));
    assert_eq!(truncate("  a  "), Some("a".to_owned()));
    assert_eq!(truncate("   "), None);
    assert_eq!(
        truncate(&"日本".repeat(40)).map(|name| name.chars().count()),
        Some(64)
    );
}

#[test]
fn a_free_name_is_kept() {
    assert_eq!(unique("Default", |_| false), "Default");
}

#[test]
fn a_taken_name_gets_the_first_free_counter() {
    let taken = ["Default", "Default (2)", "Default (3)"];
    assert_eq!(
        unique("Default", |name| taken.contains(&name)),
        "Default (4)"
    );
}

#[test]
fn a_suffixed_64_character_name_stays_within_the_limit() {
    let base = chars(64);
    let second = unique(&base, |name| name == base);
    assert_eq!(second, format!("{} (2)", chars(60)));
    assert_eq!(second.chars().count(), MAX_CHARS);
    assert_eq!(normalise(&second), Ok(second.clone()));
}

#[test]
fn a_multi_digit_counter_shortens_the_base_further() {
    let base = chars(64);
    let taken = |name: &str| name == base || name.ends_with(')') && !name.ends_with("(10)");
    let tenth = unique(&base, taken);
    assert_eq!(tenth, format!("{} (10)", chars(59)));
    assert_eq!(normalise(&tenth), Ok(tenth.clone()));
}

#[test]
fn multi_byte_names_are_cut_on_character_boundaries() {
    let base = "飛".repeat(64);
    let second = unique(&base, |name| name == base);
    assert_eq!(second, format!("{} (2)", "飛".repeat(60)));
    assert_eq!(normalise(&second), Ok(second.clone()));
}

#[test]
fn a_cut_never_leaves_a_space_before_the_suffix() {
    let base = format!("{} {}", chars(59), chars(4));
    let second = unique(&base, |name| name == base);
    assert_eq!(second, format!("{} (2)", chars(59)));
}
