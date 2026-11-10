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

//! The smallest splice that turns one list into another, so a list widget recreates only the
//! rows that changed: a row being renamed, or holding the focus, outside them is kept.

use std::ops::Range;

/// Replace `removed` items at `position` with the items of the new list in `added`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Splice {
    /// Where the change starts, in both lists.
    pub(crate) position: usize,
    /// How many items of the old list go.
    pub(crate) removed: usize,
    /// Which items of the new list come in.
    pub(crate) added: Range<usize>,
}

/// The splice between the common start and the common end of `old` and `new`; `None` when they
/// are equal.
pub(crate) fn between<T: PartialEq>(old: &[T], new: &[T]) -> Option<Splice> {
    let start = old
        .iter()
        .zip(new)
        .take_while(|(before, after)| before == after)
        .count();
    if start == old.len() && start == new.len() {
        return None;
    }
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(before, after)| before == after)
        .count();
    Some(Splice {
        position: start,
        removed: old.len() - start - end,
        added: start..new.len() - end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splice(old: &str, new: &str) -> Option<(usize, usize, String)> {
        let old: Vec<char> = old.chars().collect();
        let new: Vec<char> = new.chars().collect();
        between(&old, &new).map(|splice| {
            (
                splice.position,
                splice.removed,
                new[splice.added].iter().collect(),
            )
        })
    }

    #[test]
    fn equal_lists_need_nothing() {
        assert_eq!(splice("abc", "abc"), None);
        assert_eq!(splice("", ""), None);
    }

    #[test]
    fn an_append_touches_only_the_end() {
        assert_eq!(splice("abc", "abcd"), Some((3, 0, "d".to_owned())));
        assert_eq!(splice("", "ab"), Some((0, 0, "ab".to_owned())));
    }

    #[test]
    fn a_removal_touches_only_its_item() {
        assert_eq!(splice("abcd", "abd"), Some((2, 1, String::new())));
        assert_eq!(splice("abc", ""), Some((0, 3, String::new())));
    }

    #[test]
    fn a_move_touches_the_items_between() {
        assert_eq!(splice("abcde", "acbde"), Some((1, 2, "cb".to_owned())));
        assert_eq!(splice("abcde", "aebcd"), Some((1, 4, "ebcd".to_owned())));
    }

    #[test]
    fn repeated_items_do_not_overlap_start_and_end() {
        assert_eq!(splice("aa", "aaa"), Some((2, 0, "a".to_owned())));
        assert_eq!(splice("aaa", "aa"), Some((2, 1, String::new())));
    }
}
