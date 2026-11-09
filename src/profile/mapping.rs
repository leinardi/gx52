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

//! The per-type mode mapping: which profile each position of the mode selector uses
//! (docs/profiles.md, "Mode mapping").
//!
//! The mapping lives in two `GSettings` keys of three profile ids each. Resolving it never
//! rewrites the setting: an entry that names no usable profile of the type falls back to the
//! type's default profile until the profile comes back.

use x52::device::DeviceType;
use x52::input::Mode;

use super::ProfileId;
use super::store::ProfileStore;

/// The `GSettings` key of `device_type`'s mapping.
pub(crate) const fn settings_key(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::X52Pro => "mode-profiles-x52-pro",
        DeviceType::X52 => "mode-profiles-x52",
    }
}

/// The profile `mode` uses on a `device_type` device, from the setting's `entries`.
///
/// An entry is used when it names a loaded, usable profile of `device_type`. A missing id, a
/// profile that is not loaded, read-only or of the other type, and a setting that does not have
/// three entries, all fall back to `device_type`'s default profile.
pub(crate) fn resolve(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    mode: Mode,
    store: &ProfileStore,
) -> ProfileId {
    let fallback = || ProfileId::default_for(device_type);
    if entries.len() != Mode::ALL.len() {
        return fallback();
    }
    let index = Mode::ALL
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or(0);
    entries
        .get(index)
        .and_then(|entry| ProfileId::parse(entry.as_ref()))
        .filter(|id| {
            store
                .loaded(id)
                .is_some_and(|loaded| loaded.profile.device_type == device_type)
        })
        .unwrap_or_else(fallback)
}

#[cfg(test)]
mod tests;
