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

use std::collections::BTreeSet;
use std::fs;

use tempfile::TempDir;

use super::*;
use crate::profile::store::DefaultPersistence;
use crate::profile::tests::fixture;

const DEFAULT: &str = "x52-pro-default";

/// A store with an X52 Pro profile per `(id, name)`, an X52 profile `x52`, a read-only `newer`
/// and a `broken` file that does not load.
fn store(profiles: &[(&str, &str)]) -> (TempDir, ProfileStore) {
    let dir = tempfile::tempdir().unwrap();
    let valid = String::from_utf8(fixture("valid-x52-pro.toml")).unwrap();
    for (id, name) in profiles {
        let contents = valid.replace("name = \"Night flying\"", &format!("name = {name:?}"));
        fs::write(dir.path().join(format!("{id}.toml")), contents).unwrap();
    }
    for (id, fixture_name) in [
        ("x52", "valid-x52.toml"),
        ("newer", "newer-schema.toml"),
        ("broken", "malformed.txt"),
    ] {
        fs::write(dir.path().join(format!("{id}.toml")), fixture(fixture_name)).unwrap();
    }
    let (store, _) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
    (dir, store)
}

fn id(text: &str) -> ProfileId {
    ProfileId::parse(text).unwrap()
}

fn ids(order: &[ProfileId]) -> Vec<&str> {
    order.iter().map(ProfileId::as_str).collect()
}

/// `order` has no id twice and never the default.
fn assert_complete(order: &[ProfileId]) {
    let unique: BTreeSet<&ProfileId> = order.iter().collect();
    assert_eq!(unique.len(), order.len(), "duplicates in {order:?}");
    assert!(!order.contains(&id(DEFAULT)), "the default in {order:?}");
}

const NONE: [&str; 0] = [];

#[test]
fn the_default_comes_first_then_the_listed_then_the_rest_by_name() {
    let (_dir, store) = store(&[("aaa", "Zulu"), ("bbb", "alpha"), ("ccc", "Mike")]);
    assert_eq!(
        ids(&ordered(&NONE, DeviceType::X52Pro, &store)),
        [DEFAULT, "bbb", "ccc", "aaa"]
    );
    assert_eq!(
        ids(&ordered(&["aaa"], DeviceType::X52Pro, &store)),
        [DEFAULT, "aaa", "bbb", "ccc"]
    );
    assert_eq!(
        ids(&ordered(&["ccc", "aaa", "bbb"], DeviceType::X52Pro, &store)),
        [DEFAULT, "ccc", "aaa", "bbb"]
    );
}

#[test]
fn stale_duplicate_and_foreign_entries_are_skipped() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B")]);
    let entries = [
        "bbb",
        "gone",
        DEFAULT,
        "x52",
        "newer",
        "broken",
        "Not An Id",
        "bbb",
    ];
    assert_eq!(
        ids(&ordered(&entries, DeviceType::X52Pro, &store)),
        [DEFAULT, "bbb", "aaa"]
    );
    assert_eq!(
        ids(&ordered(&entries, DeviceType::X52, &store)),
        ["x52-default", "x52"]
    );
}

#[test]
fn an_added_profile_is_appended_to_the_complete_order() {
    // The store already holds the new profile, as after ProfileStore::add.
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B"), ("ccc", "C")]);
    let order = appended(&NONE, DeviceType::X52Pro, &store, &id("ccc"));
    assert_eq!(ids(&order), ["aaa", "bbb", "ccc"]);
    assert_complete(&order);
}

#[test]
fn a_partial_order_becomes_complete() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B"), ("ccc", "C")]);
    let order = appended(&["bbb"], DeviceType::X52Pro, &store, &id("ccc"));
    assert_eq!(ids(&order), ["bbb", "aaa", "ccc"]);
    assert_complete(&order);
}

#[test]
fn an_added_profile_lands_last_whatever_its_name() {
    let (_dir, store) = store(&[("aaa", "Bravo"), ("bbb", "Charlie"), ("new", "AAA")]);
    let order = appended(&["aaa", "bbb"], DeviceType::X52Pro, &store, &id("new"));
    assert_eq!(ids(&order), ["aaa", "bbb", "new"]);
    assert_complete(&order);
}

#[test]
fn a_duplicate_lands_right_after_its_original() {
    let (_dir, store) = store(&[("aaa", "Bravo"), ("bbb", "Charlie"), ("dup", "AAA")]);
    let order = inserted_after(
        &["aaa", "bbb"],
        DeviceType::X52Pro,
        &store,
        &id("aaa"),
        &id("dup"),
    );
    assert_eq!(ids(&order), ["aaa", "dup", "bbb"]);
    assert_complete(&order);
}

#[test]
fn a_duplicate_of_an_unlisted_profile_lands_after_it_in_the_effective_order() {
    let (_dir, store) = store(&[
        ("aaa", "Alpha"),
        ("bbb", "Bravo"),
        ("ccc", "Charlie"),
        ("dup", "AAA"),
    ]);
    // Effective order: default, ccc (listed), then aaa, bbb by name; dup, unlisted, sorts first.
    let order = inserted_after(&["ccc"], DeviceType::X52Pro, &store, &id("aaa"), &id("dup"));
    assert_eq!(ids(&order), ["ccc", "aaa", "dup", "bbb"]);
    assert_complete(&order);
}

#[test]
fn a_duplicate_of_the_default_lands_first() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B"), ("dup", "Z")]);
    let order = inserted_after(
        &["bbb", "aaa"],
        DeviceType::X52Pro,
        &store,
        &id(DEFAULT),
        &id("dup"),
    );
    assert_eq!(ids(&order), ["dup", "bbb", "aaa"]);
    assert_complete(&order);
}

#[test]
fn stale_ids_are_dropped_from_the_stored_order() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B")]);
    let entries = ["gone", "bbb", "x52", "newer", "bbb"];
    let order = appended(&entries, DeviceType::X52Pro, &store, &id("aaa"));
    assert_eq!(ids(&order), ["bbb", "aaa"]);
    assert_complete(&order);
    let order = inserted_after(
        &entries,
        DeviceType::X52Pro,
        &store,
        &id("gone"),
        &id("aaa"),
    );
    assert_eq!(
        ids(&order),
        ["bbb", "aaa"],
        "an original not in the order: last"
    );
    assert_complete(&order);
}

#[test]
fn the_settings_keys_follow_profiles_md() {
    assert_eq!(settings_key(DeviceType::X52Pro), "profile-order-x52-pro");
    assert_eq!(settings_key(DeviceType::X52), "profile-order-x52");
}

#[test]
fn a_moved_profile_takes_its_new_place() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B"), ("ccc", "C")]);
    let entries = ["aaa", "bbb", "ccc"];
    for (index, expected) in [
        (0, ["ccc", "aaa", "bbb"]),
        (1, ["aaa", "ccc", "bbb"]),
        (2, ["aaa", "bbb", "ccc"]),
        (9, ["aaa", "bbb", "ccc"]),
    ] {
        let order = moved(&entries, DeviceType::X52Pro, &store, &id("ccc"), index);
        assert_complete(&order);
        assert_eq!(ids(&order), expected, "to {index}");
    }
    let order = moved(&entries, DeviceType::X52Pro, &store, &id("aaa"), 2);
    assert_eq!(ids(&order), ["bbb", "ccc", "aaa"]);
}

#[test]
fn the_default_and_unlisted_profiles_never_move() {
    let (_dir, store) = store(&[("aaa", "A"), ("bbb", "B")]);
    let entries = ["bbb", "aaa"];
    for unmovable in [DEFAULT, "x52", "newer", "broken", "gone"] {
        let order = moved(&entries, DeviceType::X52Pro, &store, &id(unmovable), 0);
        assert_complete(&order);
        assert_eq!(ids(&order), ["bbb", "aaa"], "{unmovable}");
    }
}
