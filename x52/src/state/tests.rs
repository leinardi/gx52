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

use super::*;
use crate::clock::FixedOffset;
use crate::command::{ClockOffset, ClockTime, CommandSlot, DateFields};

fn at(instant: &str) -> Timestamp {
    instant.parse().unwrap()
}

fn zone(name: &str) -> TimeZone {
    TimeZone::get(name).unwrap()
}

fn clock1(hour: u8, minute: u8) -> Command {
    Command::Clock1 {
        time: ClockTime::new(hour, minute).unwrap(),
        h24: true,
    }
}

fn offset(clock: OffsetClock, minutes: i16) -> Command {
    Command::ClockOffset {
        clock,
        offset: ClockOffset::new(minutes).unwrap(),
        h24: true,
    }
}

fn date(first: u8, second: u8, third: u8) -> Command {
    Command::Date(DateFields::new(first, second, third).unwrap())
}

/// The slots whose command differs between `before` and `after`, which must cover the same slots.
fn changed_slots(before: &[Command], after: &[Command]) -> Vec<CommandSlot> {
    assert_eq!(before.len(), after.len(), "{before:?} / {after:?}");
    before
        .iter()
        .zip(after)
        .filter(|(old, new)| {
            assert_eq!(old.slot(), new.slot(), "the order changed");
            old != new
        })
        .map(|(old, _)| old.slot())
        .collect()
}

#[test]
fn x52_pro_resolves_every_command_in_replay_order() {
    let state = DeviceState::default_for(DeviceType::X52Pro);

    let commands = state
        .resolve(
            DeviceType::X52Pro,
            at("2026-11-03T12:00:00Z"),
            &TimeZone::UTC,
        )
        .unwrap();

    let mut expected = vec![
        Command::MfdBrightness(Brightness::FULL),
        Command::LedBrightness(Brightness::FULL),
    ];
    expected.extend(
        BiColourLed::ALL
            .iter()
            .map(|led| Command::Led(*led, LedColour::Green)),
    );
    expected.push(Command::LedSwitch(SingleColourLed::Fire, true));
    expected.push(Command::LedSwitch(SingleColourLed::Throttle, true));
    expected.extend(
        MfdLine::ALL
            .iter()
            .map(|line| Command::MfdText(*line, MfdText::EMPTY)),
    );
    expected.extend([
        clock1(12, 0),
        offset(OffsetClock::Clock2, -240),
        offset(OffsetClock::Clock3, 480),
        date(26, 11, 3),
        Command::Shift(false),
        Command::Blink(false),
    ]);
    assert_eq!(commands, expected);
}

#[test]
fn x52_resolves_no_led_shift_or_blink_command() {
    let mut state = DeviceState::default_for(DeviceType::X52Pro);
    state.shift = true;
    state.blink = true;

    let commands = state
        .resolve(DeviceType::X52, at("2026-11-03T12:00:00Z"), &TimeZone::UTC)
        .unwrap();

    let mut expected = vec![
        Command::MfdBrightness(Brightness::FULL),
        Command::LedBrightness(Brightness::FULL),
    ];
    expected.extend(
        MfdLine::ALL
            .iter()
            .map(|line| Command::MfdText(*line, MfdText::EMPTY)),
    );
    expected.extend([
        clock1(12, 0),
        offset(OffsetClock::Clock2, -240),
        offset(OffsetClock::Clock3, 480),
        date(26, 11, 3),
    ]);
    assert_eq!(commands, expected);
    assert!(
        commands
            .iter()
            .all(|command| command.supported_by(DeviceType::X52))
    );
}

#[test]
fn every_resolved_command_has_its_own_slot() {
    let mut state = DeviceState::default_for(DeviceType::X52Pro);
    state.leds.set_colour(BiColourLed::Pov2, LedColour::Amber);
    state.mfd_lines[1] = MfdText::new("GX52").unwrap();
    state.clock2.source = ZoneSource::iana("Europe/Rome").unwrap();

    for device in DeviceType::ALL {
        let commands = state
            .resolve(
                device,
                at("2026-11-03T12:00:00Z"),
                &zone("America/New_York"),
            )
            .unwrap();
        let slots: BTreeSet<CommandSlot> = commands.iter().map(Command::slot).collect();
        assert_eq!(slots.len(), commands.len(), "{device}: {commands:?}");
    }
}

#[test]
fn the_defaults_are_those_of_profiles_md() {
    let pro = DeviceState::default_for(DeviceType::X52Pro);
    assert_eq!(pro.mfd_brightness.level(), 128);
    assert_eq!(pro.led_brightness.level(), 128);
    for led in BiColourLed::ALL {
        assert_eq!(pro.leds.colour(led), LedColour::Green, "{led:?}");
    }
    for led in SingleColourLed::ALL {
        assert!(pro.leds.is_on(led), "{led:?}");
    }
    assert_eq!(
        pro.clock1,
        Clock1Settings {
            source: Clock1Source::Local,
            h24: true,
        }
    );
    assert_eq!(
        pro.clock2,
        ZoneClockSettings {
            source: ZoneSource::Fixed(FixedOffset::new(-240).unwrap()),
            h24: true,
        }
    );
    assert_eq!(
        pro.clock3,
        ZoneClockSettings {
            source: ZoneSource::Fixed(FixedOffset::new(480).unwrap()),
            h24: true,
        }
    );
    assert_eq!(pro.date_format, DateFormat::YyMmDd);
    assert!(!pro.shift);
    assert!(!pro.blink);
    assert_eq!(pro.mfd_lines, [MfdText::new("").unwrap(); 3]);

    let x52 = DeviceState::default_for(DeviceType::X52);
    assert_eq!(x52.leds, Leds::uniform(LedColour::Off, false));
    assert_eq!(
        DeviceState {
            leds: pro.leds,
            ..x52
        },
        pro,
        "only the LEDs, which the X52 lacks, differ"
    );
}

#[test]
fn one_led_change_changes_one_command() {
    let before = DeviceState::default_for(DeviceType::X52Pro);
    let mut after = before.clone();
    after.leds.set_colour(BiColourLed::T3T4, LedColour::Red);
    after.leds.set_on(SingleColourLed::Throttle, false);
    let now = at("2026-11-03T12:00:00Z");

    let changed = changed_slots(
        &before
            .resolve(DeviceType::X52Pro, now, &TimeZone::UTC)
            .unwrap(),
        &after
            .resolve(DeviceType::X52Pro, now, &TimeZone::UTC)
            .unwrap(),
    );

    assert_eq!(
        changed,
        [
            CommandSlot::BiColourLed(BiColourLed::T3T4),
            CommandSlot::SingleColourLed(SingleColourLed::Throttle),
        ]
    );
}

#[test]
fn a_minute_rollover_changes_clock_1_only() {
    let state = DeviceState::default_for(DeviceType::X52Pro);
    let resolve = |now| {
        state
            .resolve(DeviceType::X52Pro, at(now), &TimeZone::UTC)
            .unwrap()
    };

    let before = resolve("2026-11-03T12:00:00Z");
    let after = resolve("2026-11-03T12:01:00Z");

    assert_eq!(changed_slots(&before, &after), [CommandSlot::Clock1]);
    assert!(after.contains(&clock1(12, 1)));
}

#[test]
fn midnight_changes_clock_1_and_the_date() {
    let state = DeviceState::default_for(DeviceType::X52Pro);
    let resolve = |now| {
        state
            .resolve(DeviceType::X52Pro, at(now), &TimeZone::UTC)
            .unwrap()
    };

    let before = resolve("2026-11-03T23:59:00Z");
    let after = resolve("2026-11-04T00:00:00Z");

    assert_eq!(
        changed_slots(&before, &after),
        [CommandSlot::Clock1, CommandSlot::Date]
    );
    assert!(before.contains(&date(26, 11, 3)));
    assert!(after.contains(&clock1(0, 0)));
    assert!(after.contains(&date(26, 11, 4)));
}

#[test]
fn a_dst_transition_changes_clock_1_and_the_offsets() {
    // Europe/Rome moves from UTC+1 to UTC+2 at 01:00 UTC on 29 March 2026: 01:59 becomes 03:00,
    // and the fixed clocks 2 and 3 move one hour closer to (or further from) clock 1.
    let state = DeviceState::default_for(DeviceType::X52Pro);
    let rome = zone("Europe/Rome");
    let resolve = |now| state.resolve(DeviceType::X52Pro, at(now), &rome).unwrap();

    let before = resolve("2026-03-29T00:59:00Z");
    let after = resolve("2026-03-29T01:00:00Z");

    assert_eq!(
        changed_slots(&before, &after),
        [
            CommandSlot::Clock1,
            CommandSlot::ClockOffset(OffsetClock::Clock2),
            CommandSlot::ClockOffset(OffsetClock::Clock3),
        ]
    );
    assert!(before.contains(&clock1(1, 59)));
    assert!(before.contains(&offset(OffsetClock::Clock2, -300)));
    assert!(before.contains(&offset(OffsetClock::Clock3, 420)));
    assert!(after.contains(&clock1(3, 0)));
    assert!(after.contains(&offset(OffsetClock::Clock2, -360)));
    assert!(after.contains(&offset(OffsetClock::Clock3, 360)));
}
