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

//! Removed profiles whose file is not deleted yet, so the removal can be undone.
//!
//! A removal takes the entry out of the store, keeps its name reserved and tells the write
//! queue; Undo or a failed delete puts it back, a delete that succeeds frees the name. The
//! application carries out what these functions return (the order and active settings keys,
//! the toasts), so the bookkeeping is tested without GTK.

use std::collections::BTreeMap;

use x52::device::DeviceType;

use super::ProfileId;
use super::reservations::{Reservation, Reservations};
use super::store::{Entry, ProfileStore, StoreError};
use super::writes::{Command, WriteQueue};

/// A profile taken out of the store whose removal can still be undone.
#[derive(Debug)]
struct Removal {
    /// The entry, to put back on undo or when deleting its file fails.
    entry: Entry,
    /// Its name, kept from new profiles until the file is gone or the entry is back.
    reservation: Option<Reservation>,
    /// Whether it was its type's active profile, until the user makes another one active: a
    /// put-back then leaves their choice alone.
    was_active: bool,
    /// Which removal this is, so a late signal of an earlier removal's toast is ignored.
    serial: u64,
    /// Where it was in the list: after this profile (`None`: first), at this index among the
    /// profiles other than the default.
    previous: Option<ProfileId>,
    index: usize,
    /// The name or file name, for the toasts.
    title: String,
}

/// A profile just removed, for the toast and the settings keys.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Removed {
    /// The type it was listed under, if known.
    pub(crate) device_type: Option<DeviceType>,
    /// Whether it was active: the default takes its place.
    pub(crate) was_active: bool,
    /// The name or file name.
    pub(crate) title: String,
    /// Identifies this removal to [`Removals::delete_due`].
    pub(crate) serial: u64,
    /// What the write queue needs carried out.
    pub(crate) commands: Vec<Command>,
}

/// A removed profile put back in the store: where it goes in the list, and whether it is made
/// active again (when the active key still names the default).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PutBack {
    /// The type it is listed under, if known.
    pub(crate) device_type: Option<DeviceType>,
    /// The profile it was after (`None`: first).
    pub(crate) previous: Option<ProfileId>,
    /// Its index among the profiles other than the default.
    pub(crate) index: usize,
    /// Whether it was active and the user has not picked another profile since.
    pub(crate) reactivate: bool,
    /// The name or file name.
    pub(crate) title: String,
}

/// How a delete ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeleteEnd {
    /// The file is gone and the name free; drop the profile from the order of this type.
    Gone(Option<DeviceType>),
    /// The delete failed: the profile is back in the store.
    PutBack(PutBack),
    /// Nothing was pending for the profile.
    Unknown,
}

/// The removals still pending, by profile.
#[derive(Debug, Default)]
pub(crate) struct Removals {
    pending: BTreeMap<ProfileId, Removal>,
    serial: u64,
}

impl Removals {
    /// Removes `id` from `store`: reserves its name and tells `writes`. `positions` lists the
    /// profiles of its type other than the default in list order, `active` is its type's
    /// active profile.
    ///
    /// # Errors
    ///
    /// [`StoreError::DefaultNotRemovable`] and [`StoreError::Unknown`], from
    /// [`ProfileStore::take`]; nothing changes then.
    pub(crate) fn remove(
        &mut self,
        store: &mut ProfileStore,
        reservations: &mut Reservations,
        writes: &mut WriteQueue,
        id: &ProfileId,
        positions: &[ProfileId],
        active: Option<&ProfileId>,
    ) -> Result<Removed, StoreError> {
        let entry = store.take(id)?;
        let index = positions.iter().position(|listed| listed == id);
        let previous = index
            .and_then(|index| index.checked_sub(1))
            .and_then(|before| positions.get(before).cloned());
        let device_type = entry.device_type();
        // The name was the entry's own, so it is free once the entry is out.
        let reservation = device_type
            .zip(entry.name())
            .and_then(|(device_type, name)| reservations.claim(store, device_type, name));
        let was_active = active == Some(id);
        let title = entry.name().map_or_else(|| id.file_name(), str::to_owned);
        self.serial = self.serial.wrapping_add(1);
        let serial = self.serial;
        let commands = writes.removed(id);
        self.pending.insert(
            id.clone(),
            Removal {
                entry,
                reservation,
                was_active,
                serial,
                previous,
                index: index.unwrap_or(usize::MAX),
                title: title.clone(),
            },
        );
        Ok(Removed {
            device_type,
            was_active,
            title,
            serial,
            commands,
        })
    }

    /// Undoes the removal of `id`; `None` when nothing is pending for it or its delete has
    /// started.
    pub(crate) fn undo(
        &mut self,
        store: &mut ProfileStore,
        reservations: &mut Reservations,
        writes: &mut WriteQueue,
        id: &ProfileId,
    ) -> Option<(PutBack, Vec<Command>)> {
        if !self.pending.contains_key(id) {
            return None;
        }
        let commands = writes.undone(id);
        if writes.is_removing(id) {
            return None;
        }
        let removal = self.pending.remove(id)?;
        Some((put_back(store, reservations, id, removal), commands))
    }

    /// The toast of removal `serial` went away: the delete of `id` is due, unless that removal
    /// was undone (and `id` perhaps removed again since).
    pub(crate) fn delete_due(
        &self,
        writes: &mut WriteQueue,
        id: &ProfileId,
        serial: u64,
    ) -> Vec<Command> {
        match self.pending.get(id) {
            Some(removal) if removal.serial == serial => writes.delete_due(id),
            _ => Vec::new(),
        }
    }

    /// The window closes or the session ends: every pending delete is due.
    pub(crate) fn drain(&self, writes: &mut WriteQueue) -> Vec<Command> {
        self.pending
            .keys()
            .flat_map(|id| writes.delete_due(id))
            .collect()
    }

    /// The delete of `id` ended; `succeeded` says whether the file is gone.
    pub(crate) fn delete_finished(
        &mut self,
        store: &mut ProfileStore,
        reservations: &mut Reservations,
        id: &ProfileId,
        succeeded: bool,
    ) -> DeleteEnd {
        let Some(removal) = self.pending.remove(id) else {
            return DeleteEnd::Unknown;
        };
        if succeeded {
            if let Some(reservation) = removal.reservation {
                reservations.release(reservation);
            }
            DeleteEnd::Gone(removal.entry.device_type())
        } else {
            DeleteEnd::PutBack(put_back(store, reservations, id, removal))
        }
    }

    /// The user made a profile of `device_type` active: a put-back no longer makes a removed
    /// one active again.
    pub(crate) fn forget_active(&mut self, device_type: DeviceType) {
        for removal in self.pending.values_mut() {
            if removal.entry.device_type() == Some(device_type) {
                removal.was_active = false;
            }
        }
    }
}

/// Puts `removal` back into `store` and frees its name.
fn put_back(
    store: &mut ProfileStore,
    reservations: &mut Reservations,
    id: &ProfileId,
    removal: Removal,
) -> PutBack {
    let device_type = removal.entry.device_type();
    store.restore(id.clone(), removal.entry);
    if let Some(reservation) = removal.reservation {
        reservations.release(reservation);
    }
    PutBack {
        device_type,
        previous: removal.previous,
        index: removal.index,
        reactivate: removal.was_active,
        title: removal.title,
    }
}

#[cfg(test)]
mod tests;
