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
use std::ffi::OsStr;

use x52::input::InputMap;

use super::*;
use crate::device::access::AccessState;
use crate::device::{DeviceEvent, DeviceService};
use crate::profile::store::{DefaultPersistence, Entry, ProfileStore};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn without_the_option_there_is_no_run() {
    assert_eq!(parse_args(&args(&["--hidden"]), true), Ok(None));
}

#[test]
fn the_option_takes_a_directory_and_leaves_the_other_arguments() {
    let want = Request {
        out_dir: PathBuf::from("/tmp/shots"),
        remaining: args(&["--hidden"]),
    };
    assert_eq!(
        parse_args(&args(&["--screenshots", "/tmp/shots", "--hidden"]), true),
        Ok(Some(want.clone()))
    );
    assert_eq!(
        parse_args(&args(&["--hidden", "--screenshots=/tmp/shots"]), true),
        Ok(Some(want))
    );
}

#[test]
fn the_option_needs_a_directory() {
    for list in [
        &["--screenshots"][..],
        &["--screenshots="],
        &["--screenshots", "--hidden"],
    ] {
        assert_eq!(
            parse_args(&args(list), true),
            Err(ArgsError::MissingDirectory),
            "{list:?}"
        );
    }
}

#[test]
fn a_release_build_refuses_the_option() {
    assert_eq!(
        parse_args(&args(&["--screenshots", "/tmp/shots"]), false),
        Err(ArgsError::ReleaseBuild)
    );
}

#[test]
fn prepare_refuses_any_backend_but_memory_and_changes_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let request = Request {
        out_dir: scratch.path().join("shots"),
        remaining: Vec::new(),
    };
    for backend in [None, Some(OsStr::new("dconf")), Some(OsStr::new(""))] {
        assert!(
            matches!(
                prepare(&request, backend),
                Err(PrepareError::NotMemoryBackend)
            ),
            "{backend:?}"
        );
    }
    assert!(!request.out_dir.exists());
    assert!(!active());
}

#[test]
fn the_fixtures_are_four_valid_x52_pro_profiles() {
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path().join("profiles");
    write_profiles(&dir).unwrap();
    let (store, report) = ProfileStore::load(&dir, DefaultPersistence::MemoryOnly);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.ignored.is_empty(), "{:?}", report.ignored);
    assert!(report.defaults_written.is_empty());

    // The store adds the X52's default, which the X52 Pro list never shows.
    let pro: Vec<_> = store
        .entries()
        .filter_map(|(id, entry)| match entry {
            Entry::Loaded(loaded) if loaded.profile.device_type == DeviceType::X52Pro => {
                assert!(loaded.problems.is_empty(), "{id:?}: {:?}", loaded.problems);
                Some(id.as_str())
            }
            Entry::Loaded(_) => None,
            Entry::Newer(_) | Entry::Damaged(_) => panic!("{id:?} did not load"),
        })
        .collect();
    let ids: BTreeSet<&str> = pro.iter().copied().collect();
    let want: BTreeSet<&str> = PROFILES
        .iter()
        .map(|(name, _)| name.trim_end_matches(".toml"))
        .collect();
    assert_eq!(ids, want);
    assert_eq!(pro.len(), 4);
    let active = store.loaded(&active_profile().unwrap()).unwrap();
    assert_eq!(active.profile.name, "Combat Flight");
}

#[test]
fn the_mode_key_is_mode_1_on_the_x52_pro() {
    assert_eq!(
        InputMap::for_device(DeviceType::X52Pro).mode_for(backend::MODE_1_KEY),
        Some(x52::input::Mode::One)
    );
}

#[test]
fn the_fake_x52_pro_is_selected_open_and_in_mode_1() {
    let backend = ScreenshotBackend {
        local_zone: || TimeZone::UTC,
    };
    let clock = || FIXED_TIME;
    let (service, events) = DeviceService::start_with(clock, backend).unwrap();
    let (mut selected, mut ready, mut mode_1) = (false, false, false);
    while !(selected && ready && mode_1) {
        match events.recv_blocking().unwrap() {
            DeviceEvent::Selected(id) => selected = id.device_type == DeviceType::X52Pro,
            DeviceEvent::Access(access) => ready = access == AccessState::Ready,
            DeviceEvent::Mode { mode, .. } => mode_1 = mode == x52::input::Mode::One,
            DeviceEvent::Gone(_)
            | DeviceEvent::InputNode { .. }
            | DeviceEvent::Held { .. }
            | DeviceEvent::Axes { .. }
            | DeviceEvent::X52dRunning(_)
            | DeviceEvent::Error(_)
            | DeviceEvent::Sent(_)
            | DeviceEvent::Retried(_)
            | DeviceEvent::VirtualMouse(_) => {}
        }
    }
    service.shutdown();
}

#[test]
fn the_fixed_time_is_the_documented_instant() {
    assert_eq!(FIXED_TIME.to_string(), "2026-06-15T10:09:00Z");
}
