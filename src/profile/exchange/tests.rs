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
use std::path::Path;

use x52::device::{BiColourLed, LedColour};
use x52::input::Mode;

use super::*;
use crate::profile::files::{PROFILE_LIMIT, ReadError, read_limited};
use crate::profile::store::{DefaultPersistence, ProfileStore};
use crate::profile::tests::fixture;
use crate::profile::{IanaZone, MfdLineSource, ModeSets, ModeSettings, ZoneSpec};

fn store(dir: &Path) -> ProfileStore {
    ProfileStore::load(dir, DefaultPersistence::Write).0
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn custom_profile() -> Profile {
    let mut profile = Profile::builtin(DeviceType::X52Pro, "Night flying".to_owned());
    let mut mode_2 = ModeSettings::builtin(DeviceType::X52Pro);
    mode_2.leds.set_colour(BiColourLed::T1T2, LedColour::Amber);
    mode_2.mfd_lines[1] = MfdLineSource::Text("Ready".to_owned());
    mode_2.mfd_lines[2] = MfdLineSource::LastButton;
    let mode_1 = ModeSettings::builtin(DeviceType::X52Pro);
    profile.settings = ModeSets::PerMode(Box::new([mode_1.clone(), mode_2, mode_1]));
    profile.clock2.source = ZoneSpec::Iana(IanaZone::new("Asia/Kathmandu").unwrap());
    profile
}

#[test]
fn an_exported_profile_has_no_id() {
    let exported = export(&custom_profile());
    assert!(!exported.contains("id ="), "{exported}");
    assert!(exported.starts_with("schema_version = 1\n"));
}

#[test]
fn an_exported_profile_imports_back_unchanged() {
    let target = tempfile::tempdir().unwrap();
    let profile = custom_profile();
    let exported = export(&profile);

    let mut store = store(target.path());
    let preview = prepare_import(
        exported.as_bytes(),
        DeviceType::X52Pro,
        &store.names(DeviceType::X52Pro),
    )
    .unwrap();
    assert_eq!(preview.problems, []);
    assert_eq!(preview.renamed_from, None);
    assert_eq!(preview.profile, profile);
    assert!(
        !preview.needs_confirmation(),
        "a clean file is imported at once"
    );

    let id = store.add_job(&preview.profile).run().unwrap();
    assert_eq!(id.default_type(), None);
    store.added(id.clone(), preview.profile.clone());
    let (reloaded, _) = ProfileStore::load(target.path(), DefaultPersistence::Write);
    assert_eq!(reloaded.loaded(&id).unwrap().profile, profile);
    assert_eq!(store.loaded(&id).unwrap().profile, profile);
}

#[test]
fn a_file_of_the_other_type_is_refused_with_its_type() {
    for (fixture_name, expected, message) in [
        (
            "valid-x52.toml",
            DeviceType::X52Pro,
            "This is a profile for the X52",
        ),
        (
            "valid-x52-pro.toml",
            DeviceType::X52,
            "This is a profile for the X52 Pro",
        ),
    ] {
        let refusal = prepare_import(&fixture(fixture_name), expected, &[]).unwrap_err();
        assert!(
            matches!(refusal, ImportRefusal::OtherType { expected: refused, .. } if refused == expected),
            "{refusal:?}"
        );
        assert_eq!(refusal.message(), message);
    }
    let unknown = prepare_import(
        &fixture("unknown-device-type.toml"),
        DeviceType::X52Pro,
        &[],
    )
    .unwrap_err();
    assert_eq!(
        unknown.message(),
        "This is a profile for an unknown device type (x56)"
    );
}

#[test]
fn a_newer_file_is_refused_and_files_that_are_not_profiles_too() {
    let newer = prepare_import(&fixture("newer-schema.toml"), DeviceType::X52Pro, &[]).unwrap_err();
    assert!(matches!(newer, ImportRefusal::Newer(99)));
    for name in [
        "malformed.txt",
        "not-utf8.bin",
        "missing-schema-version.toml",
    ] {
        let refusal = prepare_import(&fixture(name), DeviceType::X52Pro, &[]).unwrap_err();
        assert_eq!(
            refusal.message(),
            "The file is not a GX52 profile",
            "{name}"
        );
    }
}

#[test]
fn an_oversized_file_is_not_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.toml");
    let mut contents = fixture("valid-x52-pro.toml");
    contents.resize(usize::try_from(PROFILE_LIMIT).unwrap() + 1, b'\n');
    fs::write(&path, contents).unwrap();
    let refusal = ImportRefusal::from(read_limited(&path, PROFILE_LIMIT).unwrap_err());
    assert!(matches!(
        refusal,
        ImportRefusal::Read(ReadError::TooLarge { .. })
    ));
    assert_eq!(refusal.message(), "The file cannot be read");
}

#[test]
fn field_problems_are_fixed_and_listed_before_confirming() {
    let preview = prepare_import(&fixture("out-of-range.toml"), DeviceType::X52Pro, &[]).unwrap();
    assert_eq!(preview.problems.len(), 5);
    assert!(preview.needs_confirmation());
    assert_eq!(
        preview
            .profile
            .settings_for(Mode::One)
            .led_brightness
            .level(),
        128
    );
}

#[test]
fn a_clashing_name_gets_a_suffix_within_the_limit() {
    let long = "z".repeat(64);
    let mut profile = custom_profile();
    profile.name.clone_from(&long);
    let taken = vec![long.clone(), "Night flying".to_owned()];
    let preview = prepare_import(export(&profile).as_bytes(), DeviceType::X52Pro, &taken).unwrap();
    assert_eq!(preview.profile.name, format!("{} (2)", "z".repeat(60)));
    assert_eq!(preview.renamed_from, Some(long));
    assert!(preview.needs_confirmation());
    assert_eq!(
        name::normalise(&preview.profile.name),
        Ok(preview.profile.name.clone())
    );
}

#[test]
fn a_cancelled_import_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let before = listing(dir.path());
    let entries_before: Vec<_> = store.entries().map(|(id, _)| id.clone()).collect();
    let preview = prepare_import(
        &fixture("valid-x52-pro.toml"),
        DeviceType::X52Pro,
        &store.names(DeviceType::X52Pro),
    )
    .unwrap();
    drop(preview);
    assert_eq!(listing(dir.path()), before);
    let entries_after: Vec<_> = store.entries().map(|(id, _)| id.clone()).collect();
    assert_eq!(entries_after, entries_before);
}

#[test]
fn a_name_taken_after_the_preview_is_made_unique_again() {
    let exported = export(&custom_profile());
    let mut first = prepare_import(exported.as_bytes(), DeviceType::X52Pro, &[]).unwrap();
    let second = prepare_import(exported.as_bytes(), DeviceType::X52Pro, &[]).unwrap();
    assert_eq!(first.profile.name, second.profile.name);

    // The second import was written while the first was waiting for its confirmation.
    let taken = vec![second.profile.name.clone()];
    assert!(revalidate_name(&mut first, &taken));
    assert_eq!(first.profile.name, "Night flying (2)");
    assert_eq!(first.renamed_from.as_deref(), Some("Night flying"));
    assert!(!revalidate_name(&mut first, &taken), "stable once unique");

    // The clash is gone again: the name in the file comes back.
    assert!(revalidate_name(&mut first, &[]));
    assert_eq!(first.profile.name, "Night flying");
    assert_eq!(first.renamed_from, None);
}

#[test]
fn a_clean_file_under_its_own_name_is_written_at_once() {
    let mut preview = prepare_import(
        &export(&custom_profile()).into_bytes(),
        DeviceType::X52Pro,
        &[],
    )
    .unwrap();
    assert_eq!(next_step(&mut preview, &[], false), ImportStep::Write);
}

#[test]
fn problems_ask_once_and_a_changed_name_asks_again() {
    let mut preview =
        prepare_import(&fixture("out-of-range.toml"), DeviceType::X52Pro, &[]).unwrap();
    assert_eq!(next_step(&mut preview, &[], false), ImportStep::Confirm);
    assert_eq!(
        next_step(&mut preview, &[], true),
        ImportStep::Write,
        "confirmed"
    );

    // A profile took the name while the dialog was open.
    let taken = vec![preview.profile.name.clone()];
    let name = preview.profile.name.clone();
    assert_eq!(next_step(&mut preview, &taken, true), ImportStep::Confirm);
    assert_eq!(preview.profile.name, format!("{name} (2)"));
    assert_eq!(next_step(&mut preview, &taken, true), ImportStep::Write);
}

#[test]
fn a_rename_while_the_file_was_read_turns_a_clean_import_into_a_question() {
    let profile = custom_profile();
    let mut preview =
        prepare_import(&export(&profile).into_bytes(), DeviceType::X52Pro, &[]).unwrap();
    let taken = vec![profile.name.clone()];
    assert_eq!(next_step(&mut preview, &taken, false), ImportStep::Confirm);
    assert_eq!(preview.profile.name, format!("{} (2)", profile.name));
    assert!(preview.needs_confirmation());
}
