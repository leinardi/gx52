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

//! Profile names claimed before their profile is in the store (docs/architecture.md,
//! "Persistence").
//!
//! A new, duplicated or imported profile is written off the main thread and only then added to
//! the store, and a removed profile is out of the store until its file is deleted or the removal
//! is undone. Meanwhile its name is reserved here, so no other create, import or rename can take
//! it: every name check goes through [`Reservations::taken`], the store's names and the reserved
//! ones together.

use std::collections::BTreeMap;

use x52::device::DeviceType;

use super::ProfileId;
use super::name;
use super::store::ProfileStore;

/// A reserved name, released with [`Reservations::release`].
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a reservation holds its name until it is released"]
pub(crate) struct Reservation {
    token: u64,
    name: String,
}

impl Reservation {
    /// The reserved name.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// The reserved names, per device type.
#[derive(Debug, Default)]
pub(crate) struct Reservations {
    held: BTreeMap<u64, (DeviceType, String)>,
    next: u64,
}

impl Reservations {
    /// No name reserved.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether `name` is used by a profile of `device_type` in `store` other than `except`, or
    /// reserved.
    pub(crate) fn taken(
        &self,
        store: &ProfileStore,
        device_type: DeviceType,
        name: &str,
        except: Option<&ProfileId>,
    ) -> bool {
        store.name_taken(device_type, name, except) || self.is_reserved(device_type, name)
    }

    /// Every name of `device_type` that is used or reserved, for a check off the main thread.
    pub(crate) fn names(&self, store: &ProfileStore, device_type: DeviceType) -> Vec<String> {
        let mut names = store.names(device_type);
        names.extend(
            self.held
                .values()
                .filter(|(reserved_type, _)| *reserved_type == device_type)
                .map(|(_, reserved)| reserved.clone()),
        );
        names
    }

    /// Reserves `base`, or the first free `base (2)`, `base (3)`, ... (see [`name::unique`]).
    pub(crate) fn claim_unique(
        &mut self,
        store: &ProfileStore,
        device_type: DeviceType,
        base: &str,
    ) -> Reservation {
        let unique = name::unique(base, |candidate| {
            self.taken(store, device_type, candidate, None)
        });
        self.hold(device_type, unique)
    }

    /// Reserves `name` exactly; `None` when it is used or reserved.
    pub(crate) fn claim(
        &mut self,
        store: &ProfileStore,
        device_type: DeviceType,
        name: &str,
    ) -> Option<Reservation> {
        (!self.taken(store, device_type, name, None))
            .then(|| self.hold(device_type, name.to_owned()))
    }

    /// Frees the name: its profile is in the store now, or the create failed, or the removed
    /// profile's file is deleted or the profile restored.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking it by value ends the reservation, so it cannot be released twice"
    )]
    pub(crate) fn release(&mut self, reservation: Reservation) {
        self.held.remove(&reservation.token);
    }

    fn is_reserved(&self, device_type: DeviceType, name: &str) -> bool {
        self.held
            .values()
            .any(|(reserved_type, reserved)| *reserved_type == device_type && reserved == name)
    }

    fn hold(&mut self, device_type: DeviceType, name: String) -> Reservation {
        let token = self.next;
        self.next += 1;
        self.held.insert(token, (device_type, name.clone()));
        Reservation { token, name }
    }
}

#[cfg(test)]
mod tests;
