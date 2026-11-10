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

//! The rows of the profile list (docs/ui.md, "Sidebar"): what each device type's list shows, in
//! the order it shows it.

use x52::device::DeviceType;

use super::store::{Entry, ProfileStore};
use super::{ProfileId, ProfileKind, order};

/// What a row stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    /// A profile that can be made active and edited.
    Usable(ProfileKind),
    /// A file made by a newer GX52: shown, never edited or made active.
    Newer,
    /// A file that is not a profile: shown, never edited or made active.
    Damaged,
}

/// One row of the profile list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// The profile's id.
    pub(crate) id: ProfileId,
    /// The name shown: the profile's name, or the file name when the file has none.
    pub(crate) title: String,
    /// What the row stands for.
    pub(crate) kind: RowKind,
}

/// `device_type`'s rows: the usable profiles in the order of the setting's `entries` (the
/// default first, see [`order::ordered`]), then the files made by a newer GX52 and the damaged
/// files, by file name.
pub(crate) fn rows(
    store: &ProfileStore,
    device_type: DeviceType,
    entries: &[impl AsRef<str>],
) -> Vec<Row> {
    let mut rows: Vec<Row> = order::ordered(entries, device_type, store)
        .into_iter()
        .filter_map(|id| {
            let loaded = store.loaded(&id)?;
            Some(Row {
                title: loaded.profile.name.clone(),
                kind: RowKind::Usable(loaded.profile.kind()),
                id,
            })
        })
        .collect();
    // The store lists its entries in id order, which is file name order.
    rows.extend(
        store
            .entries()
            .filter(|(_, entry)| entry.for_type(device_type))
            .filter_map(|(id, entry)| {
                let kind = match entry {
                    Entry::Loaded(_) => return None,
                    Entry::Newer(_) => RowKind::Newer,
                    Entry::Damaged(_) => RowKind::Damaged,
                };
                Some(Row {
                    id: id.clone(),
                    title: entry.name().map_or_else(|| id.file_name(), str::to_owned),
                    kind,
                })
            }),
    );
    rows
}

#[cfg(test)]
mod tests;
