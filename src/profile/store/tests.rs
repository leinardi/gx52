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

use std::collections::BTreeMap;
use std::fs;
use std::time::SystemTime;

use tempfile::TempDir;
use toml::Table;

use super::*;
use crate::profile::tests::fixture;

const ID: &str = "0f1e2d3c-aaaa-4bbb-8ccc-0123456789ab";

/// Every file of a directory with its bytes and modification time.
type Snapshot = BTreeMap<String, (Vec<u8>, SystemTime)>;

fn snapshot(dir: &Path) -> Snapshot {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            let modified = entry.metadata().unwrap().modified().unwrap();
            let name = entry.file_name().into_string().unwrap();
            (name, (fs::read(path).unwrap(), modified))
        })
        .collect()
}

fn dir_with<N: AsRef<str>, C: AsRef<[u8]>>(files: impl IntoIterator<Item = (N, C)>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        fs::write(dir.path().join(name.as_ref()), contents).unwrap();
    }
    dir
}

/// Loads `dir` without writing the defaults and checks that no file changed.
fn load_untouched(dir: &TempDir, schema: Schema) -> (ProfileStore, LoadReport) {
    let before = snapshot(dir.path());
    let loaded = ProfileStore::load_with(dir.path(), DefaultPersistence::MemoryOnly, schema);
    assert_eq!(snapshot(dir.path()), before, "loading changed a file");
    loaded
}

fn id(text: &str) -> ProfileId {
    ProfileId::parse(text).unwrap()
}

fn pro_default() -> ProfileId {
    ProfileId::default_for(DeviceType::X52Pro)
}

fn x52_default() -> ProfileId {
    ProfileId::default_for(DeviceType::X52)
}

fn file_name(text: &str) -> String {
    format!("{text}.toml")
}

fn failed_names(report: &LoadReport) -> Vec<&str> {
    report
        .failed
        .iter()
        .map(|failed| failed.file_name.as_str())
        .collect()
}

#[test]
fn files_that_are_not_profiles_are_not_loaded_and_kept() {
    let fixtures = [
        "malformed.txt",
        "not-utf8.bin",
        "missing-schema-version.toml",
        "wrong-schema-version.toml",
        "zero-schema-version.toml",
        "missing-device-type.toml",
        "unknown-device-type.toml",
    ];
    for name in fixtures {
        let dir = dir_with([(&file_name(ID), &fixture(name))]);
        let (store, report) = load_untouched(&dir, Schema::CURRENT);
        assert!(store.entry(&id(ID)).is_none(), "{name}");
        assert_eq!(failed_names(&report), [file_name(ID)], "{name}");
        assert!(
            matches!(report.failed[0].failure, FileFailure::Parse(_)),
            "{name}"
        );
    }
}

#[test]
fn a_file_over_the_size_limit_is_not_parsed() {
    let mut contents = fixture("valid-x52-pro.toml");
    contents.resize(usize::try_from(PROFILE_LIMIT).unwrap() + 1, b'\n');
    let dir = dir_with([(&file_name(ID), &contents)]);
    let (store, report) = load_untouched(&dir, Schema::CURRENT);
    assert!(store.entry(&id(ID)).is_none());
    assert!(matches!(
        report.failed[0].failure,
        FileFailure::Read(ReadError::TooLarge { .. })
    ));
}

#[test]
fn field_problems_load_the_rest_and_keep_the_file() {
    for (name, expected) in [
        ("out-of-range.toml", 5),
        ("two-mfd-lines.toml", 1),
        ("unknown-values.toml", 5),
        ("unknown-keys-x52.toml", 5),
    ] {
        let dir = dir_with([(&file_name(ID), &fixture(name))]);
        let (store, report) = load_untouched(&dir, Schema::CURRENT);
        let loaded = store.loaded(&id(ID)).unwrap();
        assert_eq!(
            loaded.problems.len(),
            expected,
            "{name}: {:?}",
            loaded.problems
        );
        assert_eq!(loaded.origin, Origin::File, "{name}");
        assert!(report.failed.is_empty(), "{name}");
    }
}

#[test]
fn duplicate_names_are_suffixed_in_memory() {
    let valid = fixture("valid-x52-pro.toml");
    let dir = dir_with([
        ("aaa.toml", &valid),
        ("bbb.toml", &valid),
        ("ccc.toml", &valid),
    ]);
    let (store, _) = load_untouched(&dir, Schema::CURRENT);
    let name = |text: &str| store.loaded(&id(text)).unwrap().profile.name.clone();
    assert_eq!(name("aaa"), "Night flying");
    assert_eq!(name("bbb"), "Night flying (2)");
    assert_eq!(name("ccc"), "Night flying (3)");
    assert_eq!(
        store.loaded(&id("bbb")).unwrap().problems,
        [FieldProblem {
            key: "name".to_owned(),
            kind: ProblemKind::DuplicateName {
                to: "Night flying (2)".to_owned()
            },
        }]
    );
}

#[test]
fn the_default_keeps_its_name_against_a_duplicate() {
    let mut other = Profile::builtin_default(DeviceType::X52);
    other.name = "Default".to_owned();
    let dir = dir_with([("aaa.toml", schema::write(&other).as_bytes())]);
    let (store, _) = load_untouched(&dir, Schema::CURRENT);
    assert_eq!(
        store.loaded(&x52_default()).unwrap().profile.name,
        "Default"
    );
    assert_eq!(
        store.loaded(&id("aaa")).unwrap().profile.name,
        "Default (2)"
    );
}

#[test]
fn names_are_unique_per_device_type_only() {
    let dir = dir_with([
        ("aaa.toml", &fixture("valid-x52-pro.toml")),
        (
            "bbb.toml",
            &fixture("valid-x52.toml").replace_name("Night flying"),
        ),
    ]);
    let (store, _) = load_untouched(&dir, Schema::CURRENT);
    assert_eq!(
        store.loaded(&id("bbb")).unwrap().profile.name,
        "Night flying"
    );
}

#[test]
fn a_newer_schema_is_listed_read_only() {
    let dir = dir_with([(&file_name(ID), &fixture("newer-schema.toml"))]);
    let (mut store, report) = load_untouched(&dir, Schema::CURRENT);
    assert_eq!(
        store.entry(&id(ID)),
        Some(&Entry::Newer(Newer {
            device_type: DeviceType::X52Pro,
            name: Some("Night flying".to_owned()),
            version: 99,
        }))
    );
    assert!(store.loaded(&id(ID)).is_none());
    assert!(report.failed.is_empty());
    let before = snapshot(dir.path());
    assert!(matches!(store.save(&id(ID)), Err(StoreError::ReadOnly(_))));
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn an_older_schema_is_migrated_and_backed_up_on_save() {
    fn noop(_: &mut Table) {}
    let original = fixture("valid-x52-pro.toml");
    let dir = dir_with([(&file_name(ID), &original)]);
    let (mut store, _) = load_untouched(&dir, Schema::new(2, &[noop]));
    assert_eq!(
        store.loaded(&id(ID)).unwrap().origin,
        Origin::Migrated { from: 1 }
    );

    store.save(&id(ID)).unwrap();
    let backup = dir.path().join(format!("{ID}.toml.schema-1.bak"));
    assert_eq!(fs::read(backup).unwrap(), original);
    assert_eq!(store.loaded(&id(ID)).unwrap().origin, Origin::File);

    // A second backup of the same version never overwrites the first.
    store.loaded_mut(&id(ID)).unwrap().origin = Origin::Migrated { from: 1 };
    store.save(&id(ID)).unwrap();
    assert!(
        dir.path()
            .join(format!("{ID}.toml.schema-1.bak-2"))
            .exists()
    );
    assert_eq!(
        fs::read(dir.path().join(format!("{ID}.toml.schema-1.bak"))).unwrap(),
        original
    );
}

#[test]
fn saving_writes_the_corrected_values() {
    let dir = dir_with([(&file_name(ID), &fixture("out-of-range.toml"))]);
    let (mut store, _) = load_untouched(&dir, Schema::CURRENT);
    store.save(&id(ID)).unwrap();
    assert!(store.loaded(&id(ID)).unwrap().problems.is_empty());
    let (reloaded, report) = load_untouched(&dir, Schema::CURRENT);
    assert!(report.failed.is_empty());
    assert_eq!(reloaded.loaded(&id(ID)).unwrap().problems, []);
}

#[test]
fn file_names_that_are_not_ids_are_ignored() {
    let valid = fixture("valid-x52-pro.toml");
    let dir = dir_with([
        ("Not An Id.toml", &valid),
        ("x.toml~", &valid),
        ("aaa.toml.rejected-20261108T091011Z", &valid),
        ("aaa.toml.schema-1.bak", &valid),
    ]);
    let (store, report) = load_untouched(&dir, Schema::CURRENT);
    assert_eq!(report.ignored, ["Not An Id.toml"]);
    assert_eq!(report.failed_files(), ["Not An Id.toml"]);
    assert_eq!(store.entries().count(), 2, "only the two defaults");
}

#[test]
fn missing_defaults_are_written_once() {
    let dir = tempfile::tempdir().unwrap();
    let (store, report) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    assert_eq!(report.defaults_written, [pro_default(), x52_default()]);
    for id in [pro_default(), x52_default()] {
        let loaded = store.loaded(&id).unwrap();
        assert_eq!(loaded.origin, Origin::File);
        let device_type = id.default_type().unwrap();
        assert_eq!(loaded.profile, Profile::builtin_default(device_type));
        let written = fs::read(store.path(&id)).unwrap();
        assert_eq!(written, schema::write(&loaded.profile).into_bytes());
    }
    let before = snapshot(dir.path());
    let (_, again) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    assert!(again.defaults_written.is_empty());
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn missing_defaults_stay_in_memory_until_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, report) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
    assert!(report.defaults_written.is_empty());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    let loaded = store.loaded(&pro_default()).unwrap();
    assert_eq!(loaded.origin, Origin::Builtin(BuiltinReason::Missing));
    // Saving creates the file.
    store.save(&pro_default()).unwrap();
    assert!(store.path(&pro_default()).exists());
}

#[test]
fn a_bad_default_file_is_kept_and_replaced_only_after_confirmation() {
    for (name, bad) in [
        ("malformed", fixture("malformed.txt")),
        ("newer", fixture("newer-schema.toml")),
        ("x52 in the pro default", fixture("valid-x52.toml")),
    ] {
        let dir = dir_with([(&pro_default().file_name(), &bad)]);
        let (mut store, report) = load_untouched(&dir, Schema::CURRENT);
        assert_eq!(failed_names(&report), [pro_default().file_name()], "{name}");
        let loaded = store.loaded(&pro_default()).unwrap();
        assert_eq!(
            loaded.origin,
            Origin::Builtin(BuiltinReason::BadFile),
            "{name}"
        );
        assert_eq!(
            loaded.profile,
            Profile::builtin_default(DeviceType::X52Pro),
            "{name}"
        );

        let before = snapshot(dir.path());
        assert!(
            matches!(
                store.save(&pro_default()),
                Err(StoreError::NeedsConfirmation(_))
            ),
            "{name}"
        );
        assert_eq!(snapshot(dir.path()), before, "{name}");

        let now: Timestamp = "2026-11-08T09:10:11Z".parse().unwrap();
        let backup = store
            .save_replacing_bad_default(&pro_default(), now)
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), bad, "{name}");
        assert!(backup.ends_with("x52-pro-default.toml.rejected-20261108T091011Z"));
        let (reloaded, report) = load_untouched(&dir, Schema::CURRENT);
        assert!(report.failed.is_empty(), "{name}");
        assert_eq!(
            reloaded.loaded(&pro_default()).unwrap().origin,
            Origin::File
        );
    }
}

#[test]
fn a_default_file_of_the_other_type_never_takes_the_fallback_id() {
    for (id, wrong) in [
        (pro_default(), fixture("valid-x52.toml")),
        (x52_default(), fixture("valid-x52-pro.toml")),
    ] {
        let dir = dir_with([(&id.file_name(), &wrong)]);
        let (store, report) = load_untouched(&dir, Schema::CURRENT);
        let expected = id.default_type().unwrap();
        assert!(matches!(
            report.failed[0].failure,
            FileFailure::WrongDefaultType { expected: reported, .. } if reported == expected
        ));
        let loaded = store.loaded(&id).unwrap();
        assert_eq!(loaded.profile.device_type, expected);
        assert_eq!(loaded.origin, Origin::Builtin(BuiltinReason::BadFile));
        // Even with persistence on, the bad file is never replaced by a load.
        let before = snapshot(dir.path());
        let _ = ProfileStore::load(dir.path(), DefaultPersistence::Write);
        let after = snapshot(dir.path());
        assert_eq!(after.get(&id.file_name()), before.get(&id.file_name()));
    }
}

#[test]
fn a_default_with_field_problems_loads_like_any_profile() {
    let dir = dir_with([(&pro_default().file_name(), &fixture("out-of-range.toml"))]);
    let (store, report) = load_untouched(&dir, Schema::CURRENT);
    assert!(report.failed.is_empty());
    let loaded = store.loaded(&pro_default()).unwrap();
    assert_eq!(loaded.origin, Origin::File);
    assert!(!loaded.problems.is_empty());
}

#[test]
fn added_profiles_get_a_random_id_and_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    let profile = Profile::builtin(DeviceType::X52, "Added".to_owned());
    let added = store.add(profile.clone()).unwrap();
    assert_eq!(added.default_type(), None);
    let (reloaded, _) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    assert_eq!(reloaded.loaded(&added).unwrap().profile, profile);
}

#[test]
fn defaults_cannot_be_removed_or_change_type() {
    let dir = tempfile::tempdir().unwrap();
    let (mut store, _) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    assert!(matches!(
        store.remove(&pro_default()),
        Err(StoreError::DefaultNotRemovable(_))
    ));
    assert!(matches!(
        store.update(&pro_default(), Profile::builtin_default(DeviceType::X52)),
        Err(StoreError::TypeChange(_))
    ));
    let added = store
        .add(Profile::builtin(DeviceType::X52, "Gone".to_owned()))
        .unwrap();
    store.remove(&added).unwrap();
    assert!(!store.path(&added).exists());
    assert!(store.entry(&added).is_none());
}

/// Replaces the `name` line of a fixture.
trait ReplaceName {
    fn replace_name(self, name: &str) -> Vec<u8>;
}

impl ReplaceName for Vec<u8> {
    fn replace_name(self, name: &str) -> Vec<u8> {
        let text = String::from_utf8(self).unwrap();
        let replaced: Vec<String> = text
            .lines()
            .map(|line| {
                if line.starts_with("name = ") {
                    format!("name = {name:?}")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        (replaced.join("\n") + "\n").into_bytes()
    }
}

#[test]
fn name_taken_looks_within_the_device_type_only() {
    let dir = dir_with([("aaa.toml", fixture("valid-x52-pro.toml"))]);
    let (store, _) = load_untouched(&dir, Schema::CURRENT);
    assert!(store.name_taken(DeviceType::X52Pro, "Night flying", None));
    assert!(!store.name_taken(DeviceType::X52Pro, "Night flying", Some(&id("aaa"))));
    assert!(!store.name_taken(DeviceType::X52, "Night flying", None));
    assert!(store.name_taken(DeviceType::X52, "Default", None));
}

#[test]
fn a_linked_profile_file_is_loaded() {
    let target = dir_with([("elsewhere.toml", fixture("valid-x52-pro.toml"))]);
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        target.path().join("elsewhere.toml"),
        dir.path().join("aaa.toml"),
    )
    .unwrap();
    let (store, report) = load_untouched(&dir, Schema::CURRENT);
    assert!(report.failed.is_empty());
    assert_eq!(
        store.loaded(&id("aaa")).unwrap().profile.name,
        "Night flying"
    );
}
