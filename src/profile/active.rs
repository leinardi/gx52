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

//! The active profile of each device type: the one the device shows (docs/profiles.md, "Active
//! profile").
//!
//! The active profile lives in one `GSettings` key per device type. Resolving it never rewrites
//! the setting: an id that names no usable profile of the type falls back to the type's default
//! profile until the profile comes back.

use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::DeviceState;

use super::store::ProfileStore;
use super::{LiveValues, ProfileId};

/// The `GSettings` key of `device_type`'s active profile.
pub(crate) const fn settings_key(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::X52Pro => "active-profile-x52-pro",
        DeviceType::X52 => "active-profile-x52",
    }
}

/// The profile a `device_type` device shows, from the setting's `id`.
///
/// The id is used when it names a loaded, usable profile of `device_type`. A missing id, a
/// profile that is not loaded, read-only or of the other type all fall back to `device_type`'s
/// default profile.
pub(crate) fn resolve(id: &str, device_type: DeviceType, store: &ProfileStore) -> ProfileId {
    ProfileId::parse(id)
        .filter(|id| {
            store
                .loaded(id)
                .is_some_and(|loaded| loaded.profile.device_type == device_type)
        })
        .unwrap_or_else(|| ProfileId::default_for(device_type))
}

/// The desired state of `device_type`'s active profile in `mode`, from the setting's
/// `active_entry`, with the dynamic MFD lines filled in from `live`; `None` when the profile is
/// not usable.
///
/// Needs no device: the editor's preview resolves the shown type with it while no device is
/// connected, and the device gets the connected type's state from it.
pub(crate) fn state_for(
    store: &ProfileStore,
    active_entry: &str,
    device_type: DeviceType,
    mode: Mode,
    live: &LiveValues,
) -> Option<(ProfileId, DeviceState)> {
    let id = resolve(active_entry, device_type, store);
    let state = store.loaded(&id)?.profile.to_device_state(mode, live);
    Some((id, state))
}

#[cfg(test)]
mod tests;
