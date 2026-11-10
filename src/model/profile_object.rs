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

//! One row of the profile list as a `GObject`, built from a [`Row`] of the store.

use std::cell::{Cell, RefCell};

use gtk::glib;
use x52::input::Mode;

use crate::profile::list::{Row, RowKind};
use crate::profile::{ProfileId, ProfileKind};

/// What a profile of the list can do.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "GX52ProfileState")]
pub enum ProfileState {
    /// It can be made active and edited.
    #[default]
    Usable,
    /// Made by a newer GX52: shown only.
    Newer,
    /// Not a profile: shown only.
    Damaged,
}

#[expect(
    clippy::min_ident_chars,
    reason = "the glib::Properties derive names its closure arguments with one letter"
)]
mod imp {
    use super::{Cell, ProfileState, RefCell, glib};
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::ProfileObject)]
    pub struct ProfileObject {
        /// The profile's id.
        #[property(get, construct_only)]
        pub(super) id: RefCell<String>,
        /// The name shown: the profile's name, or its file name when the file has none.
        #[property(get, set)]
        pub(super) title: RefCell<String>,
        /// What the profile can do.
        #[property(get, set, builder(ProfileState::Usable))]
        pub(super) state: Cell<ProfileState>,
        /// Whether it has a set of settings per mode.
        #[property(get, set)]
        pub(super) per_mode: Cell<bool>,
        /// Whether it is its device type's default profile.
        #[property(get, construct_only)]
        pub(super) is_default: Cell<bool>,
        /// Whether it is the active profile, the one on the device.
        #[property(get, set)]
        pub(super) active: Cell<bool>,
        /// The mode whose settings the device shows, `1`–`3`, while this per-mode profile is
        /// active and the mode is known; `0` otherwise.
        #[property(get, set, maximum = 3)]
        pub(super) mode_shown: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileObject {
        const NAME: &'static str = "GX52ProfileObject";
        type Type = super::ProfileObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ProfileObject {}
}

glib::wrapper! {
    pub struct ProfileObject(ObjectSubclass<imp::ProfileObject>);
}

impl ProfileObject {
    /// The object of `row`.
    pub(crate) fn new(row: &Row) -> Self {
        let object: Self = glib::Object::builder()
            .property("id", row.id.as_str())
            .property("is-default", row.id.default_type().is_some())
            .build();
        object.update(row);
        object
    }

    /// The profile's id.
    pub(crate) fn profile_id(&self) -> Option<ProfileId> {
        ProfileId::parse(&self.id())
    }

    /// Takes the values of `row`, a row of the same profile; notifies only what changed.
    pub(crate) fn update(&self, row: &Row) {
        if self.title() != row.title {
            self.set_title(row.title.as_str());
        }
        let (state, per_mode) = match row.kind {
            RowKind::Usable(kind) => (ProfileState::Usable, kind == ProfileKind::PerMode),
            RowKind::Newer => (ProfileState::Newer, false),
            RowKind::Damaged => (ProfileState::Damaged, false),
        };
        if self.state() != state {
            self.set_state(state);
        }
        if self.per_mode() != per_mode {
            self.set_per_mode(per_mode);
        }
    }

    /// Marks the profile active or not, with the mode the device shows while it is a per-mode
    /// active profile.
    pub(crate) fn show_active(&self, active: bool, mode: Option<Mode>) {
        if self.active() != active {
            self.set_active(active);
        }
        let mode_shown = match mode {
            Some(mode) if active && self.per_mode() => mode_number(mode),
            _ => 0,
        };
        if self.mode_shown() != mode_shown {
            self.set_mode_shown(mode_shown);
        }
    }

    /// Whether it can be made active and edited.
    pub(crate) fn is_usable(&self) -> bool {
        self.state() == ProfileState::Usable
    }
}

/// The number of `mode`, `1`–`3`.
const fn mode_number(mode: Mode) -> u32 {
    match mode {
        Mode::One => 1,
        Mode::Two => 2,
        Mode::Three => 3,
    }
}
