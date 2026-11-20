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

//! The entry of a text the MFD shows on one line, shared by the editors that edit one: the row
//! keeps a typed or pasted text within the line, and its suffix shows the bytes used and the
//! characters the MFD replaces (docs/ui.md, "MFD view").

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;

use crate::profile::mfd::TextInfo;

/// The part of a typed or pasted `inserted` that an entry lets in between `before` and `after`,
/// the text on each side of the insertion point.
pub(crate) type InsertPolicy = fn(before: &str, inserted: &str, after: &str) -> String;

/// An entry row for an MFD text, with its counter and substitutions suffixes.
#[derive(Debug)]
pub(crate) struct MfdEntry {
    /// The row the text is typed in.
    row: adw::EntryRow,
    /// "n/16".
    counter: gtk::Label,
    /// The characters the MFD shows as a replacement.
    substitutions: gtk::Label,
}

/// The byte offset of character `position` in `text`, or the end.
fn byte_offset(text: &str, position: i32) -> usize {
    usize::try_from(position)
        .ok()
        .and_then(|position| text.char_indices().nth(position))
        .map_or(text.len(), |(index, _)| index)
}

impl MfdEntry {
    /// Adds the suffixes to `row` and lets in only what `policy` accepts of each insertion,
    /// except while `syncing` is true: the text an editor shows itself is set whole.
    pub(crate) fn new(
        row: adw::EntryRow,
        policy: InsertPolicy,
        syncing: impl Fn() -> bool + 'static,
    ) -> Self {
        // Up to 16 replaced characters: cut, so the row fits the narrowest window. The label's
        // text, which screen readers read, keeps the whole list.
        let substitutions = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(12)
            .tooltip_text(gettext(
                "These characters are not on the MFD and are shown as their replacement",
            ))
            .css_classes(["caption", "warning"])
            .build();
        let counter = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .css_classes(["caption", "numeric", "dim-label"])
            .build();
        row.add_suffix(&substitutions);
        row.add_suffix(&counter);
        // Typing goes to the entry's text widget, which emits the insertion: only the part the
        // policy accepts is let through, so the text already there and the cursor stay as they
        // are. The guard keeps the handler out of its own insertion of that part.
        if let Some(delegate) = row.delegate() {
            let inserting = Rc::new(Cell::new(false));
            delegate.connect_insert_text(move |delegate, inserted, position| {
                if syncing() || inserting.get() {
                    return;
                }
                let current = delegate.text();
                let (before, after) = current.split_at(byte_offset(&current, *position));
                let accepted = policy(before, inserted, after);
                if accepted == inserted {
                    return;
                }
                if !accepted.is_empty() {
                    inserting.set(true);
                    delegate.insert_text(&accepted, position);
                    inserting.set(false);
                }
                delegate.stop_signal_emission_by_name("insert-text");
            });
        }
        Self {
            row,
            counter,
            substitutions,
        }
    }

    /// The row the text is typed in.
    pub(crate) fn row(&self) -> &adw::EntryRow {
        &self.row
    }

    /// Shows the counter and the substitutions of `text`, the text the row holds.
    pub(crate) fn show_info(&self, text: &str) {
        let info = TextInfo::of(text);
        self.counter.set_label(&info.counter());
        self.counter
            .update_property(&[gtk::accessible::Property::Label(&info.counter_label())]);
        self.substitutions.set_label(&info.substitutions);
        self.substitutions
            .set_visible(!info.substitutions.is_empty());
    }
}
