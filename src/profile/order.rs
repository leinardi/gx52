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

//! The order of each device type's profiles in the profile list (docs/profiles.md, "Profile
//! order").
//!
//! The order lives in one `GSettings` key per device type: the ids of the non-default profiles in
//! the order the user set. The setting may be incomplete or stale, so every function here works
//! on the **effective** order, the one the list shows, and a mutation returns the complete list
//! to store. Writing the setting is the caller's job.

use x52::device::DeviceType;

use super::ProfileId;
use super::store::ProfileStore;

/// The `GSettings` key of `device_type`'s profile order.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the profile list of M5 reads and writes the order"
    )
)]
pub(crate) const fn settings_key(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::X52Pro => "profile-order-x52-pro",
        DeviceType::X52 => "profile-order-x52",
    }
}

/// The effective order of `device_type`'s loaded profiles, from the setting's `entries`.
///
/// The default profile comes first. The listed ids that name a loaded profile of the type follow
/// in their order, each once; the loaded profiles the setting does not list come last, by name
/// (then by id, so the order is total).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the profile list of M5 shows the order")
)]
pub(crate) fn ordered(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    store: &ProfileStore,
) -> Vec<ProfileId> {
    let default = ProfileId::default_for(device_type);
    let usable = |id: &ProfileId| {
        *id != default
            && store
                .loaded(id)
                .is_some_and(|loaded| loaded.profile.device_type == device_type)
    };
    let mut order = vec![default.clone()];
    for id in entries
        .iter()
        .filter_map(|entry| ProfileId::parse(entry.as_ref()))
    {
        if usable(&id) && !order.contains(&id) {
            order.push(id);
        }
    }
    let mut unlisted: Vec<(String, &ProfileId)> = store
        .entries()
        .map(|(id, _)| id)
        .filter(|id| usable(id) && !order.contains(id))
        .filter_map(|id| Some((store.loaded(id)?.profile.name.to_lowercase(), id)))
        .collect();
    unlisted.sort();
    order.extend(unlisted.into_iter().map(|(_, id)| id.clone()));
    order
}

/// The setting to store after `id` was added at the end (a new or imported profile).
///
/// `id` is normally already in `store`, so the effective order lists it among the unlisted
/// profiles; it is moved to the end. The result is complete: every loaded non-default profile,
/// in the effective order, without the default.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the profile list of M5 adds profiles")
)]
pub(crate) fn appended(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    store: &ProfileStore,
    id: &ProfileId,
) -> Vec<ProfileId> {
    let mut order = without(entries, device_type, store, id);
    if *id != ProfileId::default_for(device_type) {
        order.push(id.clone());
    }
    order
}

/// The setting to store after `id` was made as a duplicate of `original`: right after it, or
/// first when `original` is the default profile (or not in the order at all, then last).
///
/// The result is complete, as for [`appended`].
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the profile list of M5 duplicates profiles")
)]
pub(crate) fn inserted_after(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    store: &ProfileStore,
    original: &ProfileId,
    id: &ProfileId,
) -> Vec<ProfileId> {
    let default = ProfileId::default_for(device_type);
    let mut order = without(entries, device_type, store, id);
    if *id == default {
        return order;
    }
    let position = if *original == default {
        0
    } else {
        order
            .iter()
            .position(|listed| listed == original)
            .map_or(order.len(), |index| index + 1)
    };
    order.insert(position, id.clone());
    order
}

/// The setting to store after `id` was moved to `index` of the non-default profiles (`0` is right
/// after the default profile); an index past the end moves it last. The default profile never
/// moves: for it the effective order is returned unchanged.
///
/// The result is complete, as for [`appended`].
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the profile list of M5 reorders profiles")
)]
pub(crate) fn moved(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    store: &ProfileStore,
    id: &ProfileId,
    index: usize,
) -> Vec<ProfileId> {
    let mut order = without(entries, device_type, store, id);
    let listed = ordered(entries, device_type, store).contains(id);
    if listed && *id != ProfileId::default_for(device_type) {
        order.insert(index.min(order.len()), id.clone());
    }
    order
}

/// The effective order without the default profile and without `id`.
fn without(
    entries: &[impl AsRef<str>],
    device_type: DeviceType,
    store: &ProfileStore,
    id: &ProfileId,
) -> Vec<ProfileId> {
    let default = ProfileId::default_for(device_type);
    ordered(entries, device_type, store)
        .into_iter()
        .filter(|listed| *listed != default && listed != id)
        .collect()
}

#[cfg(test)]
mod tests;
