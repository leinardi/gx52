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

//! Translatable Rust sources keep each string on one line. The `.pot` is extracted from them as C
//! (Meson's `glib` preset), and C keeps the indentation after a `\` line continuation that Rust
//! drops, so such a string's msgid never matches the text the app looks up: it is never
//! translated.

use std::path::Path;

use meta::repo;

/// Whether `line` ends in a string continuation: one `\` at the end, not an escaped `\\`.
fn continues(line: &str) -> bool {
    let trailing = line.len() - line.trim_end_matches('\\').len();
    trailing % 2 == 1
}

#[test]
fn translatable_sources_have_no_string_continuations() {
    let sources: Vec<String> = repo::read("po/POTFILES.in")
        .lines()
        .filter(|line| {
            Path::new(line)
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .map(str::to_owned)
        .collect();
    assert!(
        !sources.is_empty(),
        "po/POTFILES.in lists no Rust file; the check would pass on nothing"
    );
    let problems: Vec<String> = sources
        .iter()
        .flat_map(|file| {
            repo::read(file)
                .lines()
                .enumerate()
                .filter(|(_, line)| continues(line.trim_end()))
                .map(|(index, _)| format!("{file}:{}", index + 1))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        problems.is_empty(),
        "string continued with `\\`, which xgettext extracts with its indentation; keep it on one \
         line:\n{}",
        problems.join("\n")
    );
}

#[test]
fn only_an_unescaped_trailing_backslash_continues() {
    assert!(continues(r#"    "it may \"#));
    assert!(!continues(r#"    "C:\\"#));
    assert!(!continues(r#"    "no continuation","#));
}
