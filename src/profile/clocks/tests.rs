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

use super::*;

fn target(profile: &str, clock: OffsetClock) -> ZoneTarget {
    ZoneTarget {
        profile: ProfileId::parse(profile).unwrap(),
        clock,
    }
}

const PROFILE_A: &str = "0f1e2d3c-aaaa-4bbb-8ccc-0123456789aa";
const PROFILE_B: &str = "0f1e2d3c-aaaa-4bbb-8ccc-0123456789bb";

#[test]
fn offsets_are_written_with_a_true_minus() {
    assert_eq!(offset_text(345), "UTC+05:45");
    assert_eq!(offset_text(-210), "UTC\u{2212}03:30");
    assert_eq!(offset_text(0), "UTC+00:00");
    assert_eq!(offset_text(-720), "UTC\u{2212}12:00");
    assert_eq!(offset_text(840), "UTC+14:00");
}

#[test]
fn every_offset_reads_back_as_written() {
    for minutes in -720..=840 {
        assert_eq!(
            parse_offset(&offset_text(minutes)),
            Some(minutes),
            "{minutes}"
        );
    }
}

#[test]
fn offsets_are_read_as_typed() {
    for (text, expected) in [
        ("+5:45", Some(345)),
        ("5:45", Some(345)),
        ("5", Some(300)),
        ("-3", Some(-180)),
        ("\u{2212}03:30", Some(-210)),
        ("utc -3:30", Some(-210)),
        (" UTC+14 ", Some(840)),
        ("UTC", Some(0)),
        ("", Some(0)),
        ("+5:60", None),
        ("5:5:5", None),
        ("abc", None),
        ("+123", None),
        ("--3", None),
        ("UTC+ ", None),
    ] {
        assert_eq!(parse_offset(text), expected, "{text:?}");
    }
}

#[test]
fn a_zone_left_becomes_its_offset_rounded_to_15_minutes() {
    let minutes = |seconds| fixed_from_seconds(seconds).unwrap().minutes();
    assert_eq!(minutes(9 * 3600), 540, "Tokyo");
    assert_eq!(minutes(5 * 3600 + 45 * 60), 345, "Kathmandu");
    assert_eq!(minutes(-(3 * 3600 + 30 * 60)), -210, "St John's");
    assert_eq!(minutes(7 * 60 + 29), 0, "under half a step rounds down");
    assert_eq!(minutes(7 * 60 + 30), 15, "half a step rounds up");
    assert_eq!(minutes(-(7 * 60 + 31)), -15);
    assert_eq!(minutes(15 * 3600), 840, "clamped to UTC+14:00");
    assert_eq!(minutes(-13 * 3600), -720, "clamped to UTC−12:00");
}

#[test]
fn a_zone_left_follows_its_daylight_saving_time() {
    let rome = TimeZone::get("Europe/Rome").unwrap();
    let winter: Timestamp = "2026-01-15T12:00:00Z".parse().unwrap();
    let summer: Timestamp = "2026-07-15T12:00:00Z".parse().unwrap();
    assert_eq!(fixed_from_zone(&rome, winter).unwrap().minutes(), 60);
    assert_eq!(fixed_from_zone(&rome, summer).unwrap().minutes(), 120);
    assert_eq!(utc_offset_minutes(&rome, summer), 120);
}

#[test]
fn a_remembered_zone_stays_with_its_profile_and_clock() {
    let mut memory = ZoneMemory::default();
    let tokyo = IanaZone::new("Asia/Tokyo").unwrap();
    memory.remember(target(PROFILE_A, OffsetClock::Clock2), tokyo.clone());
    assert_eq!(
        memory.recall(&target(PROFILE_A, OffsetClock::Clock2)),
        Some(&tokyo)
    );
    assert_eq!(
        memory.recall(&target(PROFILE_B, OffsetClock::Clock2)),
        None,
        "another profile's clock 2 never gets it"
    );
    assert_eq!(
        memory.recall(&target(PROFILE_A, OffsetClock::Clock3)),
        None,
        "the profile's other clock never gets it"
    );
    let rome = IanaZone::new("Europe/Rome").unwrap();
    memory.remember(target(PROFILE_A, OffsetClock::Clock2), rome.clone());
    assert_eq!(
        memory.recall(&target(PROFILE_A, OffsetClock::Clock2)),
        Some(&rome)
    );
}

#[test]
fn a_pick_completes_while_the_dialog_is_open_for_its_clock() {
    let mut requests = ZoneRequests::default();
    assert_eq!(requests.pick(), None, "nothing to pick for while closed");
    requests.open(target(PROFILE_A, OffsetClock::Clock2));
    let request = requests.pick().unwrap();
    assert_eq!(request.target, target(PROFILE_A, OffsetClock::Clock2));
    assert!(requests.is_current(&request));
}

#[test]
fn a_late_pick_after_a_profile_switch_is_dropped() {
    let mut requests = ZoneRequests::default();
    requests.open(target(PROFILE_A, OffsetClock::Clock2));
    let request = requests.pick().unwrap();
    // The shown profile changes: the dialog closes, then opens again for PROFILE_B.
    requests.close();
    requests.open(target(PROFILE_B, OffsetClock::Clock2));
    assert!(
        !requests.is_current(&request),
        "PROFILE_A's pick never reaches PROFILE_B"
    );
}

#[test]
fn a_late_pick_after_a_close_is_dropped() {
    let mut requests = ZoneRequests::default();
    requests.open(target(PROFILE_A, OffsetClock::Clock3));
    let request = requests.pick().unwrap();
    requests.close();
    assert!(!requests.is_current(&request));
    assert_eq!(requests.target(), None);
}

#[test]
fn only_the_latest_of_two_picks_completes() {
    let mut requests = ZoneRequests::default();
    requests.open(target(PROFILE_A, OffsetClock::Clock2));
    let first = requests.pick().unwrap();
    let second = requests.pick().unwrap();
    assert!(!requests.is_current(&first));
    assert!(requests.is_current(&second));
}

#[test]
fn a_dialog_opened_again_for_the_same_clock_drops_the_old_pick() {
    let mut requests = ZoneRequests::default();
    requests.open(target(PROFILE_A, OffsetClock::Clock2));
    let request = requests.pick().unwrap();
    requests.close();
    requests.open(target(PROFILE_A, OffsetClock::Clock2));
    assert!(!requests.is_current(&request));
}
