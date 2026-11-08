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

//! Profile names: the length rule and the `(2)`, `(3)`, ... suffix that makes a name unique within
//! its device type, shared by loading, the legacy import and the file import.

/// The longest name, in characters, after trimming.
pub(crate) const MAX_CHARS: usize = 64;

/// Why a name cannot be used as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NameProblem {
    /// Nothing is left after trimming.
    #[error("the name is empty")]
    Empty,
    /// More than [`MAX_CHARS`] characters after trimming.
    #[error("the name has {0} characters, more than {MAX_CHARS}")]
    TooLong(usize),
}

/// `raw` trimmed, when it has 1–[`MAX_CHARS`] characters.
pub(crate) fn normalise(raw: &str) -> Result<String, NameProblem> {
    let trimmed = raw.trim();
    match trimmed.chars().count() {
        0 => Err(NameProblem::Empty),
        count if count > MAX_CHARS => Err(NameProblem::TooLong(count)),
        _ => Ok(trimmed.to_owned()),
    }
}

/// `raw` trimmed and cut to [`MAX_CHARS`] characters, or `None` when nothing is left: the legacy
/// import keeps a long name instead of replacing it.
pub(crate) fn truncate(raw: &str) -> Option<String> {
    let cut = cut(raw.trim(), MAX_CHARS);
    (!cut.is_empty()).then_some(cut)
}

/// `base`, or the first of `base (2)`, `base (3)`, ... that `taken` does not report, cut so the
/// result never has more than [`MAX_CHARS`] characters.
///
/// `base` must already be a valid name (see [`normalise`]).
pub(crate) fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..=u32::MAX)
        .map(|counter| {
            let suffix = format!(" ({counter})");
            let room = MAX_CHARS.saturating_sub(suffix.chars().count());
            format!("{}{suffix}", cut(base, room))
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// The first `max` characters of `text`, without trailing whitespace.
fn cut(text: &str, max: usize) -> String {
    let end = text
        .char_indices()
        .nth(max)
        .map_or(text.len(), |(index, _)| index);
    text.get(..end).unwrap_or(text).trim_end().to_owned()
}

#[cfg(test)]
mod tests;
