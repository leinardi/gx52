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

use jiff::tz::{self, TimeZone};
use jiff::{SignedDuration, Timestamp};

use super::*;

fn at(instant: &str) -> Timestamp {
    instant.parse().unwrap()
}

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap()
}

fn fixed(minutes: i16) -> ZoneSource {
    ZoneSource::Fixed(FixedOffset::new(minutes).unwrap())
}

/// The minutes clock 2 is sent for `source` at `now`, clock 1 on `clock1_source` in `local`.
fn offset_minutes(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    source: &ZoneSource,
) -> i16 {
    let result =
        offset_command(now, local, clock1_source, OffsetClock::Clock2, source, true).unwrap();
    let Command::ClockOffset { offset, .. } = result else {
        panic!("not a clock offset: {result:?}");
    };
    offset.minutes()
}

/// The hour and minute clock 1 is sent at `now`.
fn clock1_time(now: Timestamp, local: &TimeZone, source: Clock1Source) -> (u8, u8) {
    let result = clock1_command(now, local, source, true).unwrap();
    let Command::Clock1 { time, .. } = result else {
        panic!("not clock 1: {result:?}");
    };
    (time.hour(), time.minute())
}

fn date_fields(
    now: Timestamp,
    local: &TimeZone,
    source: Clock1Source,
    format: DateFormat,
) -> (u8, u8, u8) {
    let result = date_command(now, local, source, format).unwrap();
    let Command::Date(fields) = result else {
        panic!("not a date: {result:?}");
    };
    (fields.first(), fields.second(), fields.third())
}

#[test]
fn fake_clock_is_set_and_advanced() {
    let clock = FakeClock::new(at("2026-01-15T12:00:00Z"));
    clock.advance(SignedDuration::from_secs(90));
    assert_eq!(clock.now(), at("2026-01-15T12:01:30Z"));
    clock.set(at("2026-07-15T12:00:00Z"));
    assert_eq!(clock.now(), at("2026-07-15T12:00:00Z"));
    clock.advance(SignedDuration::MAX);
    assert_eq!(clock.now(), Timestamp::MAX);
}

#[test]
fn system_clock_reads_the_present() {
    // After 2026-01-01, and not frozen at the epoch.
    assert!(SystemClock.now() > at("2026-01-01T00:00:00Z"));
}

#[test]
fn a_closure_is_a_clock() {
    let clock = || at("2026-01-15T12:00:00Z");
    assert_eq!(Clock::now(&clock), at("2026-01-15T12:00:00Z"));
}

#[test]
fn profiles_worked_examples_in_winter_and_summer() {
    // Clock 1 on local time in Central Europe: UTC+1 in winter, UTC+2 in summer.
    let rome = zone("Europe/Rome");
    let winter = FakeClock::new(at("2026-01-15T12:00:00Z"));
    let summer = FakeClock::new(at("2026-07-15T12:00:00Z"));
    for (legacy, in_winter, in_summer) in [(-240, -300, -360), (480, 420, 360), (345, 285, 225)] {
        let source = fixed(legacy);
        assert_eq!(
            offset_minutes(winter.now(), &rome, Clock1Source::Local, &source),
            in_winter,
            "{legacy}"
        );
        assert_eq!(
            offset_minutes(summer.now(), &rome, Clock1Source::Local, &source),
            in_summer,
            "{legacy}"
        );
    }
}

#[test]
fn protocol_normalisation_example() {
    // Clock 1 at UTC−5, clock 2 at UTC+14: +1140 minutes, sent as −300.
    let minus_five = TimeZone::fixed(tz::offset(-5));
    let now = at("2026-01-15T12:00:00Z");
    assert_eq!(
        offset_minutes(now, &minus_five, Clock1Source::Local, &fixed(840)),
        -300
    );
    // And the other way round: clock 1 at UTC+14, clock 2 at UTC−12 is −1560, sent as −120.
    let plus_fourteen = TimeZone::fixed(tz::offset(14));
    assert_eq!(
        offset_minutes(now, &plus_fourteen, Clock1Source::Local, &fixed(-720)),
        -120
    );
}

#[test]
fn normalised_offsets_stay_within_half_a_day() {
    for minutes in -3000..=3000 {
        let normalised = normalise(minutes).unwrap();
        assert!(
            (-719..=720).contains(&normalised),
            "{minutes} gave {normalised}"
        );
        assert_eq!(
            (i32::from(normalised) - minutes).rem_euclid(1440),
            0,
            "{minutes}"
        );
    }
}

#[test]
fn clock_1_on_utc_ignores_the_local_zone() {
    let rome = zone("Europe/Rome");
    let now = at("2026-07-15T12:34:56Z");
    assert_eq!(clock1_time(now, &rome, Clock1Source::Utc), (12, 34));
    assert_eq!(clock1_time(now, &rome, Clock1Source::Local), (14, 34));
    // Clocks 2 and 3 are then offsets from UTC.
    assert_eq!(
        offset_minutes(now, &rome, Clock1Source::Utc, &fixed(480)),
        480
    );
    assert_eq!(
        offset_minutes(
            now,
            &rome,
            Clock1Source::Utc,
            &ZoneSource::Zone(rome.clone())
        ),
        120
    );
}

#[test]
fn clock_1_h24_flag_is_passed_through() {
    let now = at("2026-07-15T12:34:56Z");
    for h24 in [true, false] {
        let command = clock1_command(now, &TimeZone::UTC, Clock1Source::Utc, h24).unwrap();
        assert!(matches!(command, Command::Clock1 { h24: flag, .. } if flag == h24));
    }
}

#[test]
fn rome_spring_forward_and_fall_back() {
    let rome = zone("Europe/Rome");
    let utc_clock = fixed(0);
    // 2026-03-29 01:00 UTC: 02:00 CET becomes 03:00 CEST.
    let before = at("2026-03-29T00:59:30Z");
    let after = at("2026-03-29T01:00:30Z");
    assert_eq!(clock1_time(before, &rome, Clock1Source::Local), (1, 59));
    assert_eq!(clock1_time(after, &rome, Clock1Source::Local), (3, 0));
    assert_eq!(
        offset_minutes(before, &rome, Clock1Source::Local, &utc_clock),
        -60
    );
    assert_eq!(
        offset_minutes(after, &rome, Clock1Source::Local, &utc_clock),
        -120
    );
    // 2026-10-25 01:00 UTC: 03:00 CEST becomes 02:00 CET.
    let before = at("2026-10-25T00:59:30Z");
    let after = at("2026-10-25T01:00:30Z");
    assert_eq!(clock1_time(before, &rome, Clock1Source::Local), (2, 59));
    assert_eq!(clock1_time(after, &rome, Clock1Source::Local), (2, 0));
    assert_eq!(
        offset_minutes(before, &rome, Clock1Source::Local, &utc_clock),
        -120
    );
    assert_eq!(
        offset_minutes(after, &rome, Clock1Source::Local, &utc_clock),
        -60
    );
}

#[test]
fn new_york_spring_forward_and_fall_back_on_clock_2() {
    let new_york = ZoneSource::iana("America/New_York").unwrap();
    let rome = zone("Europe/Rome");
    // 2026-03-08 07:00 UTC: EST (−5) becomes EDT (−4); clock 1 on UTC.
    assert_eq!(
        offset_minutes(
            at("2026-03-08T06:59:30Z"),
            &rome,
            Clock1Source::Utc,
            &new_york
        ),
        -300
    );
    assert_eq!(
        offset_minutes(
            at("2026-03-08T07:00:30Z"),
            &rome,
            Clock1Source::Utc,
            &new_york
        ),
        -240
    );
    // 2026-11-01 06:00 UTC: EDT becomes EST.
    assert_eq!(
        offset_minutes(
            at("2026-11-01T05:59:30Z"),
            &rome,
            Clock1Source::Utc,
            &new_york
        ),
        -240
    );
    assert_eq!(
        offset_minutes(
            at("2026-11-01T06:00:30Z"),
            &rome,
            Clock1Source::Utc,
            &new_york
        ),
        -300
    );
    // With clock 1 on Rome time, between the two zones' transitions (Rome still on CET, New York
    // already on EDT): −240 − 60.
    assert_eq!(
        offset_minutes(
            at("2026-03-15T12:00:00Z"),
            &rome,
            Clock1Source::Local,
            &new_york
        ),
        -300
    );
}

#[test]
fn next_resend_is_the_next_minute_boundary() {
    let rome = zone("Europe/Rome");
    let new_york = ZoneSource::iana("America/New_York").unwrap();
    let sources = [&new_york, &fixed(480)];
    let next = |now| next_resend(at(now), &rome, Clock1Source::Local, &sources).unwrap();
    assert_eq!(next("2026-01-15T12:00:30Z"), at("2026-01-15T12:01:00Z"));
    // On a boundary, the next one.
    assert_eq!(next("2026-01-15T12:00:00Z"), at("2026-01-15T12:01:00Z"));
    assert_eq!(
        next("2026-01-15T12:00:59.999999999Z"),
        at("2026-01-15T12:01:00Z")
    );
    // Before the epoch, rounding still goes forward.
    assert_eq!(next("1969-12-31T23:59:30.5Z"), at("1970-01-01T00:00:00Z"));
}

#[test]
fn next_resend_lands_on_the_dst_transitions_on_either_side() {
    let rome = zone("Europe/Rome");
    let new_york = ZoneSource::iana("America/New_York").unwrap();
    // Clock 1's zone (Rome) and clock 2's (New York): each transition is a resend instant, and
    // the instant after it is the next minute.
    let local_sources: [&ZoneSource; 0] = [];
    let utc_sources = [&new_york];
    for (now, local, source, sources, want) in [
        (
            "2026-03-29T00:59:30Z",
            &rome,
            Clock1Source::Local,
            &local_sources[..],
            "2026-03-29T01:00:00Z",
        ),
        (
            "2026-03-29T01:00:00Z",
            &rome,
            Clock1Source::Local,
            &local_sources[..],
            "2026-03-29T01:01:00Z",
        ),
        (
            "2026-10-25T00:59:30Z",
            &rome,
            Clock1Source::Local,
            &local_sources[..],
            "2026-10-25T01:00:00Z",
        ),
        (
            "2026-10-25T01:00:00Z",
            &rome,
            Clock1Source::Local,
            &local_sources[..],
            "2026-10-25T01:01:00Z",
        ),
        (
            "2026-03-08T06:59:30Z",
            &rome,
            Clock1Source::Utc,
            &utc_sources[..],
            "2026-03-08T07:00:00Z",
        ),
        (
            "2026-03-08T07:00:00Z",
            &rome,
            Clock1Source::Utc,
            &utc_sources[..],
            "2026-03-08T07:01:00Z",
        ),
        (
            "2026-11-01T05:59:30Z",
            &rome,
            Clock1Source::Utc,
            &utc_sources[..],
            "2026-11-01T06:00:00Z",
        ),
        (
            "2026-11-01T06:00:00Z",
            &rome,
            Clock1Source::Utc,
            &utc_sources[..],
            "2026-11-01T06:01:00Z",
        ),
    ] {
        assert_eq!(
            next_resend(at(now), local, source, sources).unwrap(),
            at(want),
            "{now}"
        );
    }
}

#[test]
fn next_resend_takes_a_transition_before_the_minute() {
    // A zone whose transitions are 30 seconds past the minute (2026-03-29 and 2026-10-25), so the
    // transition comes before the next minute boundary.
    let odd = TimeZone::posix("XST0XDT,M3.5.0/0:00:30,M10.5.0/0:00:30").unwrap();
    let odd_source = ZoneSource::Zone(odd.clone());
    let none: [&ZoneSource; 0] = [];
    let now = at("2026-03-29T00:00:10Z");
    assert_eq!(
        next_resend(now, &odd, Clock1Source::Local, &none).unwrap(),
        at("2026-03-29T00:00:30Z")
    );
    assert_eq!(
        next_resend(now, &TimeZone::UTC, Clock1Source::Utc, &[&odd_source]).unwrap(),
        at("2026-03-29T00:00:30Z")
    );
    // Clock 1 on UTC ignores the local zone's transitions.
    assert_eq!(
        next_resend(now, &odd, Clock1Source::Utc, &none).unwrap(),
        at("2026-03-29T00:01:00Z")
    );
    // A fixed offset has no transitions.
    assert_eq!(
        next_resend(now, &TimeZone::UTC, Clock1Source::Utc, &[&fixed(60)]).unwrap(),
        at("2026-03-29T00:01:00Z")
    );
}

#[test]
fn date_formats_put_the_fields_in_their_order() {
    let rome = zone("Europe/Rome");
    let now = at("2026-10-28T12:00:00Z");
    assert_eq!(
        date_fields(now, &rome, Clock1Source::Local, DateFormat::DdMmYy),
        (28, 10, 26)
    );
    assert_eq!(
        date_fields(now, &rome, Clock1Source::Local, DateFormat::MmDdYy),
        (10, 28, 26)
    );
    assert_eq!(
        date_fields(now, &rome, Clock1Source::Local, DateFormat::YyMmDd),
        (26, 10, 28)
    );
}

#[test]
fn the_date_follows_clock_1s_zone() {
    let rome = zone("Europe/Rome");
    // 23:30 UTC on the 27th is 00:30 on the 28th in Rome.
    let now = at("2026-10-27T23:30:00Z");
    assert_eq!(
        date_fields(now, &rome, Clock1Source::Utc, DateFormat::DdMmYy),
        (27, 10, 26)
    );
    assert_eq!(
        date_fields(now, &rome, Clock1Source::Local, DateFormat::DdMmYy),
        (28, 10, 26)
    );
    // The year is two digits, 2000 included.
    assert_eq!(
        date_fields(
            at("2000-01-01T12:00:00Z"),
            &rome,
            Clock1Source::Utc,
            DateFormat::YyMmDd
        ),
        (0, 1, 1)
    );
}

#[test]
fn fixed_offsets_are_minus_720_to_plus_840() {
    assert_eq!(FixedOffset::new(-720).map(FixedOffset::minutes), Ok(-720));
    assert_eq!(FixedOffset::new(840).map(FixedOffset::minutes), Ok(840));
    assert_eq!(
        FixedOffset::new(-721),
        Err(RangeError::OutOfRange {
            field: "fixed offset",
            value: -721,
            min: -720,
            max: 840
        })
    );
    assert!(FixedOffset::new(841).is_err());
}

#[test]
fn an_unknown_zone_is_refused() {
    let error = ZoneSource::iana("Not/A_Zone").err();
    assert!(
        matches!(&error, Some(ClockError::UnknownZone { name, .. }) if name == "Not/A_Zone"),
        "{error:?}"
    );
    assert!(ZoneSource::iana("Europe/Rome").is_ok());
}

#[test]
fn the_ends_of_the_timestamp_range_never_panic() {
    let rome = zone("Europe/Rome");
    let new_york = ZoneSource::iana("America/New_York").unwrap();
    let one_minute_before_max = Timestamp::MAX
        .checked_sub(SignedDuration::from_mins(1))
        .unwrap();
    for now in [Timestamp::MIN, one_minute_before_max, Timestamp::MAX] {
        for source in [Clock1Source::Local, Clock1Source::Utc] {
            for result in [
                clock1_command(now, &rome, source, true),
                offset_command(now, &rome, source, OffsetClock::Clock3, &new_york, false),
                offset_command(now, &rome, source, OffsetClock::Clock2, &fixed(840), false),
                date_command(now, &rome, source, DateFormat::YyMmDd),
            ] {
                assert!(
                    matches!(result, Ok(_) | Err(ClockError::OutOfRange { .. })),
                    "{now} {source:?}: {result:?}"
                );
            }
            let resend = next_resend(now, &rome, source, &[&new_york]);
            assert!(
                matches!(resend, Ok(_) | Err(ClockError::OutOfRange { .. })),
                "{now} {source:?}: {resend:?}"
            );
        }
    }
    // The last whole minute has no next one; the minute before it does, and Timestamp::MIN too.
    let none: [&ZoneSource; 0] = [];
    assert!(matches!(
        next_resend(Timestamp::MAX, &TimeZone::UTC, Clock1Source::Utc, &none),
        Err(ClockError::OutOfRange { .. })
    ));
    assert!(
        next_resend(
            one_minute_before_max,
            &TimeZone::UTC,
            Clock1Source::Utc,
            &none
        )
        .is_ok()
    );
    assert!(next_resend(Timestamp::MIN, &TimeZone::UTC, Clock1Source::Utc, &none).is_ok());
}
