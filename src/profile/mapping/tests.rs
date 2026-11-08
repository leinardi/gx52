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

fn pro(entries: &[&str], mode: Mode, store: &ProfileStore) -> String {
    resolve(entries, DeviceType::X52Pro, mode, store).to_string()
}

#[test]
fn each_mode_uses_its_entry() {
    let (_dir, store) = store();
    let entries = [PRO, "x52-pro-default", PRO];
    assert_eq!(pro(&entries, Mode::One, &store), PRO);
    assert_eq!(pro(&entries, Mode::Two, &store), "x52-pro-default");
    assert_eq!(pro(&entries, Mode::Three, &store), PRO);
}

#[test]
fn the_default_mapping_uses_the_default_profile() {
    let (_dir, store) = store();
    for device_type in DeviceType::ALL {
        let default = ProfileId::default_for(device_type).to_string();
        let entries = [default.as_str(); 3];
        for mode in Mode::ALL {
            assert_eq!(
                resolve(&entries, device_type, mode, &store).to_string(),
                default
            );
        }
    }
}

#[test]
fn entries_without_a_usable_profile_of_the_type_fall_back_to_the_default() {
    let (_dir, store) = store();
    for (entry, why) in [
        (NON_PRO, "a profile of the other type"),
        ("x52-default", "the other type's default"),
        (NEWER, "a read-only profile"),
        (BROKEN, "a file that did not load"),
        ("eee", "a removed profile"),
        ("Not An Id", "not an id"),
        ("", "empty"),
    ] {
        assert_eq!(
            pro(&[entry, PRO, PRO], Mode::One, &store),
            "x52-pro-default",
            "{why}"
        );
        assert_eq!(
            pro(&[entry, PRO, PRO], Mode::Two, &store),
            PRO,
            "{why}: other entries"
        );
    }
}

#[test]
fn a_mapping_without_three_entries_falls_back_for_every_mode() {
    let (_dir, store) = store();
    for entries in [
        &[][..],
        &[PRO][..],
        &[PRO, PRO][..],
        &[PRO, PRO, PRO, PRO][..],
    ] {
        for mode in Mode::ALL {
            assert_eq!(pro(entries, mode, &store), "x52-pro-default", "{entries:?}");
        }
    }
}

#[test]
fn the_x52_mapping_resolves_x52_profiles() {
    let (_dir, store) = store();
    let entries = [NON_PRO, PRO, "x52-default"];
    let x52 = |mode| resolve(&entries, DeviceType::X52, mode, &store).to_string();
    assert_eq!(x52(Mode::One), NON_PRO);
    assert_eq!(x52(Mode::Two), "x52-default");
    assert_eq!(x52(Mode::Three), "x52-default");
}

#[test]
fn the_settings_keys_follow_profiles_md() {
    assert_eq!(settings_key(DeviceType::X52Pro), "mode-profiles-x52-pro");
    assert_eq!(settings_key(DeviceType::X52), "mode-profiles-x52");
}
