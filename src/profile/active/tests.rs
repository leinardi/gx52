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

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::profile::store::DefaultPersistence;
use crate::profile::tests::fixture;

const PRO: &str = "aaa";
const NON_PRO: &str = "bbb";
const NEWER: &str = "ccc";
const BROKEN: &str = "ddd";

fn store() -> (TempDir, ProfileStore) {
    let dir = tempfile::tempdir().unwrap();
    for (id, fixture_name) in [
        (PRO, "valid-x52-pro.toml"),
        (NON_PRO, "valid-x52.toml"),
        (NEWER, "newer-schema.toml"),
        (BROKEN, "malformed.txt"),
    ] {
        fs::write(dir.path().join(format!("{id}.toml")), fixture(fixture_name)).unwrap();
    }
    let (store, _) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
    (dir, store)
}

#[test]
fn a_loaded_profile_of_the_type_is_used() {
    let (_dir, store) = store();
    assert_eq!(resolve(PRO, DeviceType::X52Pro, &store).as_str(), PRO);
    assert_eq!(resolve(NON_PRO, DeviceType::X52, &store).as_str(), NON_PRO);
}

#[test]
fn the_default_settings_use_the_default_profile() {
    let (_dir, store) = store();
    for device_type in DeviceType::ALL {
        let default = ProfileId::default_for(device_type);
        assert_eq!(resolve(default.as_str(), device_type, &store), default);
    }
}

#[test]
fn ids_without_a_usable_profile_of_the_type_fall_back_to_the_default() {
    let (_dir, store) = store();
    for (id, why) in [
        (NON_PRO, "a profile of the other type"),
        ("x52-default", "the other type's default"),
        (NEWER, "a read-only profile"),
        (BROKEN, "a file that did not load"),
        ("eee", "a removed profile"),
        ("Not An Id", "not an id"),
        ("", "empty"),
    ] {
        assert_eq!(
            resolve(id, DeviceType::X52Pro, &store).as_str(),
            "x52-pro-default",
            "{why}"
        );
    }
    assert_eq!(
        resolve(PRO, DeviceType::X52, &store).as_str(),
        "x52-default",
        "an X52 Pro profile on the X52"
    );
}

#[test]
fn the_settings_keys_follow_profiles_md() {
    assert_eq!(settings_key(DeviceType::X52Pro), "active-profile-x52-pro");
    assert_eq!(settings_key(DeviceType::X52), "active-profile-x52");
}

#[test]
fn the_state_follows_edits_with_no_device() {
    let (_dir, mut store) = store();
    let live = LiveValues::default();
    for (id, device_type) in [(PRO, DeviceType::X52Pro), (NON_PRO, DeviceType::X52)] {
        let id = ProfileId::parse(id).unwrap();
        let mut profile = store.loaded(&id).unwrap().profile.clone();
        let settings = profile.settings.for_mode_mut(Mode::One);
        settings.mfd_brightness = x52::command::Brightness::new(7).unwrap();
        settings.mfd_lines[0] = crate::profile::MfdLineSource::ProfileName;
        store.update(&id, profile.clone()).unwrap();

        let (resolved, state) =
            state_for(&store, id.as_str(), device_type, Mode::One, &live).unwrap();
        assert_eq!(resolved, id, "{device_type}");
        assert_eq!(state.mfd_brightness.level(), 7, "{device_type}");
        assert_eq!(
            state.mfd_lines[0],
            x52::charset::MfdText::fit(&profile.name),
            "{device_type}"
        );
    }
}

#[test]
fn the_default_profile_has_a_state_without_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
    let live = LiveValues::default();
    // The default profiles are in memory even with no file.
    assert!(state_for(&store, "", DeviceType::X52Pro, Mode::One, &live).is_some());
}
