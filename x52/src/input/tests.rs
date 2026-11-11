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
use crate::test_support::testdata;

/// One `EV_KEY` event of an evtest capture: the code, pressed or released, and the timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyEvent {
    code: u16,
    pressed: bool,
    at: Duration,
}

/// The `EV_KEY` press and release events of an evtest capture, in order. A line looks like
/// `Event: time 1793716445.329008, type 1 (EV_KEY), code 715 (BTN_TRIGGER_HAPPY12), value 0`;
/// every other line (the header, scan codes) is skipped.
fn key_events(capture: &str) -> Vec<KeyEvent> {
    capture
        .lines()
        .filter_map(|line| line.strip_prefix("Event: time "))
        .filter_map(|line| {
            let mut fields = line.split(", ");
            let time = fields.next()?;
            if fields.next()? != "type 1 (EV_KEY)" {
                return None;
            }
            let code = fields.next()?.strip_prefix("code ")?.split(' ').next()?;
            let value = fields.next()?.strip_prefix("value ")?;
            let (seconds, micros) = time.split_once('.')?;
            Some(KeyEvent {
                code: code.parse().unwrap(),
                pressed: match value {
                    "1" => true,
                    "0" => false,
                    other => panic!("unexpected key value {other} in {line:?}"),
                },
                at: Duration::from_secs(seconds.parse().unwrap())
                    + Duration::from_micros(micros.parse().unwrap()),
            })
        })
        .collect()
}

/// The modes applied while replaying `events` through `map` into `debouncer`, the pending one
/// included at the end (as the reader's poll timeout would).
fn replay(map: &InputMap, debouncer: &mut ModeDebouncer, events: &[KeyEvent]) -> Vec<Mode> {
    let mut applied: Vec<Mode> = events
        .iter()
        .filter_map(|event| {
            let mode = map.mode_for(event.code)?;
            debouncer.on_event(mode, event.pressed, event.at)
        })
        .collect();
    if let Some(deadline) = debouncer.deadline() {
        applied.extend(debouncer.advance_to(deadline));
    }
    applied
}

fn ms(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

#[test]
fn the_recorded_bounce_gives_1_2_3_1_three_times() {
    let capture = String::from_utf8(testdata("input/mode-bounce.evtest")).unwrap();
    let events = key_events(&capture);
    let first = events.first().unwrap();
    // The capture starts in mode 1 (its first mode event is the release of 715); the snapshot is
    // taken just before it.
    assert_eq!((first.code, first.pressed), (715, false));
    let map = InputMap::for_device(DeviceType::X52Pro);
    let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::One), first.at);
    assert_eq!(debouncer.active(), Some(Mode::One));
    let applied = replay(&map, &mut debouncer, &events);
    // Pass 1 and 2: 2, 3, 1. Pass 3: mode 2 was held for only 160 ms on the way, so 3 follows 1.
    // From 3 to 1 the selector passes through 2 every time, and 2 is never applied there.
    assert_eq!(
        applied,
        [
            Mode::Two,
            Mode::Three,
            Mode::One,
            Mode::Two,
            Mode::Three,
            Mode::One,
            Mode::Three,
            Mode::One
        ]
    );
    assert!(
        applied
            .windows(2)
            .all(|pair| pair != [Mode::Three, Mode::Two]),
        "{applied:?}"
    );
}

#[test]
fn the_recording_bounces() {
    // The fixture shows what the debounce is for: presses shorter than 200 ms and gaps with no
    // mode held.
    let capture = String::from_utf8(testdata("input/mode-bounce.evtest")).unwrap();
    let events: Vec<KeyEvent> = key_events(&capture)
        .into_iter()
        .filter(|event| (715..=717).contains(&event.code))
        .collect();
    assert_eq!(events.len(), 64);
    let short_press = events.windows(2).any(|pair| {
        pair[0].code == pair[1].code
            && pair[0].pressed
            && !pair[1].pressed
            && pair[1].at.checked_sub(pair[0].at).unwrap() < SETTLE
    });
    assert!(short_press);
}

/// The mode button codes of a device, by mode.
fn mode_code(device: DeviceType, mode: Mode) -> u16 {
    let map = InputMap::for_device(device);
    map.buttons()
        .find(|(code, _)| map.mode_for(*code) == Some(mode))
        .map(|(code, _)| code)
        .unwrap()
}

/// A synthetic sequence of `(mode, pressed, milliseconds)` as raw codes of `device`.
fn raw(device: DeviceType, sequence: &[(Mode, bool, u64)]) -> Vec<KeyEvent> {
    sequence
        .iter()
        .map(|(mode, pressed, at)| KeyEvent {
            code: mode_code(device, *mode),
            pressed: *pressed,
            at: ms(*at),
        })
        .collect()
}

/// A synthetic case: what it shows, the `(mode, pressed, milliseconds)` sequence from mode 1 at
/// time 0, and the modes applied.
type Case = (&'static str, Vec<(Mode, bool, u64)>, Vec<Mode>);

fn transition_cases() -> Vec<Case> {
    use Mode::{One, Three, Two};
    vec![
        (
            "a press shorter than 200 ms is not applied",
            vec![
                (One, false, 10),
                (Two, true, 20),
                (Two, false, 219),
                (One, true, 230),
            ],
            vec![],
        ),
        (
            "a press of 200 ms is applied",
            vec![(One, false, 10), (Two, true, 20), (Two, false, 220)],
            vec![Two],
        ),
        (
            "two modes held are not applied, the one left alone is after 200 ms",
            vec![
                (One, false, 10),
                (Two, true, 20),
                (Three, true, 30),
                (Two, false, 300),
                (Three, false, 499),
                (Three, true, 600),
            ],
            vec![Three],
        ),
        (
            "a gap with nothing held keeps the mode",
            vec![(One, false, 10), (One, true, 80), (One, false, 400)],
            vec![],
        ),
        (
            "3 to 1 through 2",
            vec![
                (One, false, 10),
                (Three, true, 20),
                (Three, false, 400),
                (Two, true, 420),
                (Two, false, 480),
                (One, true, 500),
            ],
            vec![Three, One],
        ),
        (
            "bouncing on one position applies it once",
            vec![
                (One, false, 10),
                (Two, true, 20),
                (Two, false, 40),
                (Two, true, 50),
                (Two, false, 60),
                (Two, true, 70),
            ],
            vec![Two],
        ),
    ]
}

#[test]
fn synthetic_transitions_are_the_same_on_both_devices() {
    for device in DeviceType::ALL {
        let map = InputMap::for_device(device);
        for (case, sequence, want) in transition_cases() {
            let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::One), ms(0));
            let events = raw(device, &sequence);
            assert_eq!(
                replay(&map, &mut debouncer, &events),
                want,
                "{device} {case}"
            );
        }
    }
}

#[test]
fn the_deadline_and_advance_path() {
    for device in DeviceType::ALL {
        let map = InputMap::for_device(device);
        let two = mode_code(device, Mode::Two);
        let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::One), ms(0));
        assert_eq!(debouncer.deadline(), None);
        let mode = map.mode_for(mode_code(device, Mode::One)).unwrap();
        assert_eq!(debouncer.on_event(mode, false, ms(10)), None);
        let mode = map.mode_for(two).unwrap();
        assert_eq!(debouncer.on_event(mode, true, ms(20)), None);
        assert_eq!(debouncer.deadline(), Some(ms(220)), "{device}");
        // Nothing happens before the deadline, the mode is applied at it, and only once.
        assert_eq!(debouncer.advance_to(ms(219)), None);
        assert_eq!(debouncer.active(), Some(Mode::One));
        assert_eq!(debouncer.advance_to(ms(220)), Some(Mode::Two));
        assert_eq!(debouncer.active(), Some(Mode::Two));
        assert_eq!(debouncer.deadline(), None);
        assert_eq!(debouncer.advance_to(ms(1000)), None);
    }
}

#[test]
fn start_stop_on_the_x52_never_changes_the_mode() {
    let map = InputMap::for_device(DeviceType::X52);
    assert_eq!(map.button(715), Some(Button::StartStop));
    assert_eq!(map.mode_for(715), None);
    let pro = InputMap::for_device(DeviceType::X52Pro);
    assert_eq!(pro.mode_for(715), Some(Mode::One));
    // A capture of the Pro's mode buttons read with the X52's map changes nothing.
    let events = raw(
        DeviceType::X52Pro,
        &[(Mode::One, false, 10), (Mode::One, true, 20)],
    );
    let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::Two), ms(0));
    assert_eq!(replay(&map, &mut debouncer, &events), []);
    assert_eq!(debouncer.active(), Some(Mode::Two));
}

#[test]
fn start_up_with_one_mode_held_is_active_at_once() {
    let debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::Three), ms(5));
    assert_eq!(debouncer.active(), Some(Mode::Three));
    assert_eq!(debouncer.deadline(), None);
}

#[test]
fn start_up_with_two_modes_held_waits_for_one_alone() {
    let held: ModeSet = [Mode::One, Mode::Two].into_iter().collect();
    let mut debouncer = ModeDebouncer::new(held, ms(0));
    assert_eq!(debouncer.active(), None);
    assert_eq!(debouncer.deadline(), None);
    // 1 released: 2 is alone from now, and applied 200 ms later.
    assert_eq!(debouncer.on_event(Mode::One, false, ms(50)), None);
    assert_eq!(debouncer.deadline(), Some(ms(250)));
    assert_eq!(debouncer.advance_to(ms(249)), None);
    assert_eq!(debouncer.advance_to(ms(250)), Some(Mode::Two));
    assert_eq!(debouncer.active(), Some(Mode::Two));
}

#[test]
fn start_up_with_no_mode_held_waits_for_the_first() {
    let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY, ms(0));
    assert_eq!(debouncer.active(), None);
    assert_eq!(debouncer.on_event(Mode::Three, true, ms(10)), None);
    assert_eq!(debouncer.active(), None);
    // The release after 300 ms applies it.
    assert_eq!(
        debouncer.on_event(Mode::Three, false, ms(310)),
        Some(Mode::Three)
    );
    assert_eq!(debouncer.active(), Some(Mode::Three));
}

#[test]
fn replayed_events_from_before_the_snapshot_are_harmless() {
    // The snapshot has mode 2 held; the press of 2 queued before it changes nothing.
    let mut debouncer = ModeDebouncer::new(ModeSet::EMPTY.with(Mode::Two), ms(100));
    assert_eq!(debouncer.on_event(Mode::Two, true, ms(50)), None);
    assert_eq!(debouncer.on_event(Mode::One, false, ms(60)), None);
    assert_eq!(debouncer.deadline(), None);
    assert_eq!(debouncer.active(), Some(Mode::Two));
}

#[test]
fn mode_sets() {
    let set: ModeSet = [Mode::One, Mode::Three].into_iter().collect();
    assert!(set.contains(Mode::One) && !set.contains(Mode::Two) && set.contains(Mode::Three));
    assert_eq!(set.sole(), None);
    assert_eq!(set.without(Mode::One).sole(), Some(Mode::Three));
    assert!(set.without(Mode::One).without(Mode::Three).is_empty());
    assert_eq!(ModeSet::EMPTY.sole(), None);
}

#[test]
fn every_code_in_each_map_is_unique() {
    for device in DeviceType::ALL {
        let map = InputMap::for_device(device);
        let codes: Vec<u16> = map.buttons().map(|(code, _)| code).collect();
        let unique: BTreeSet<u16> = codes.iter().copied().collect();
        assert_eq!(codes.len(), unique.len(), "{device} buttons");
        let buttons: BTreeSet<Button> = map.buttons().map(|(_, button)| button).collect();
        assert_eq!(
            codes.len(),
            buttons.len(),
            "{device}: a button has two codes"
        );
        let codes: Vec<u16> = map.axes().map(|(code, _)| code).collect();
        let unique: BTreeSet<u16> = codes.iter().copied().collect();
        assert_eq!(codes.len(), unique.len(), "{device} axes");
    }
}

#[test]
fn the_pro_map_matches_the_protocol() {
    // docs/protocol.md, "Buttons" and "Axes", row by row.
    use Button::*;
    let map = InputMap::for_device(DeviceType::X52Pro);
    assert!(map.is_verified());
    let buttons: Vec<(u16, Button)> = map.buttons().collect();
    assert_eq!(
        buttons,
        [
            (288, TriggerStage1),
            (289, Fire),
            (290, ButtonA),
            (291, ButtonB),
            (292, ButtonC),
            (293, Pinkie),
            (294, ButtonD),
            (295, ButtonE),
            (296, T1),
            (297, T2),
            (298, T3),
            (299, T4),
            (300, T5),
            (301, T6),
            (302, TriggerStage2),
            (303, MouseButton),
            (704, ScrollDown),
            (705, ScrollUp),
            (706, ScrollPress),
            (707, Pov2Up),
            (708, Pov2Right),
            (709, Pov2Down),
            (710, Pov2Left),
            (711, ThrottleHatUp),
            (712, ThrottleHatRight),
            (713, ThrottleHatDown),
            (714, ThrottleHatLeft),
            (715, Mode1),
            (716, Mode2),
            (717, Mode3),
            (718, Clutch),
            (719, Function),
            (720, StartStop),
            (721, Reset),
            (722, PageUp),
            (723, PageDown),
            (724, Up),
            (725, Down),
            (726, Select),
        ]
    );
    // ABS_X 0x00, ABS_Y 0x01, ABS_Z 0x02, ABS_RX 0x03, ABS_RY 0x04, ABS_RZ 0x05,
    // ABS_THROTTLE 0x06, ABS_HAT0X 0x10, ABS_HAT0Y 0x11, ABS_MISC 0x28 and 41.
    let axes: Vec<(u16, Axis)> = map.axes().collect();
    assert_eq!(
        axes,
        [
            (0x00, Axis::StickX),
            (0x01, Axis::StickY),
            (0x02, Axis::Throttle),
            (0x03, Axis::RotaryI),
            (0x04, Axis::RotaryE),
            (0x05, Axis::Twist),
            (0x06, Axis::Slider),
            (0x10, Axis::Pov1X),
            (0x11, Axis::Pov1Y),
            (0x28, Axis::MinistickX),
            (0x29, Axis::MinistickY),
        ]
    );
    assert_eq!(map.mode_for(716), Some(Mode::Two));
    assert_eq!(map.button(304), None);
}

#[test]
fn the_x52_map_is_the_unverified_candidate() {
    let map = InputMap::for_device(DeviceType::X52);
    assert!(!map.is_verified());
    for (code, mode) in [(711, Mode::One), (712, Mode::Two), (713, Mode::Three)] {
        assert_eq!(map.mode_for(code), Some(mode));
    }
    assert_eq!(map.button(303), Some(Button::Pov2Up));
    assert_eq!(map.button(717), Some(Button::Clutch));
    assert_eq!(map.button(722), None);
    // No ministick: its codes on the X52 are not known.
    assert_eq!(map.axis(0x28), None);
    assert_eq!(map.axis(0x29), None);
    assert_eq!(map.axis(0x00), Some(Axis::StickX));
}

fn button(button: Button) -> Input {
    Input::Button(button)
}

const POV1_UP: Input = Input::Pov1(Pov1Direction::Up);
const POV1_RIGHT: Input = Input::Pov1(Pov1Direction::Right);

#[test]
fn the_last_input_is_the_newest_still_held() {
    let mut held = HeldInputs::new();
    assert_eq!(held.last(), None);
    held.press(button(Button::Fire));
    held.press(button(Button::ButtonA));
    assert_eq!(held.last(), Some(button(Button::ButtonA)));
    held.release(button(Button::ButtonA));
    assert_eq!(held.last(), Some(button(Button::Fire)), "falls back");
    held.release(button(Button::Fire));
    assert_eq!(held.last(), None);
}

#[test]
fn a_repeated_press_keeps_one_entry_in_its_place() {
    let mut held = HeldInputs::new();
    held.press(button(Button::Fire));
    held.press(button(Button::ButtonA));
    held.press(button(Button::Fire));
    assert_eq!(held.last(), Some(button(Button::ButtonA)), "not reordered");
    held.release(button(Button::Fire));
    assert_eq!(held.last(), Some(button(Button::ButtonA)));
    held.release(button(Button::ButtonA));
    assert_eq!(held.last(), None, "one entry each");
}

#[test]
fn the_mode_selector_is_never_held() {
    let mut held = HeldInputs::new();
    held.press(button(Button::Fire));
    for mode in [Button::Mode1, Button::Mode2, Button::Mode3] {
        held.press(button(mode));
        assert_eq!(held.last(), Some(button(Button::Fire)), "{mode:?}");
    }
    held.release(button(Button::Fire));
    assert_eq!(held.last(), None);
}

#[test]
fn pov1_releases_only_the_directions_that_left() {
    let mut held = HeldInputs::new();
    held.set_pov1(0, -1);
    held.set_pov1(1, -1);
    assert_eq!(held.last(), Some(POV1_RIGHT));
    held.press(button(Button::Fire));
    // Right released, up still held: Fire stays the newest.
    held.set_pov1(0, -1);
    assert_eq!(held.last(), Some(button(Button::Fire)));
    held.release(button(Button::Fire));
    assert_eq!(held.last(), Some(POV1_UP));
    held.set_pov1(0, 0);
    assert_eq!(held.last(), None);
}

#[test]
fn an_unchanged_pov1_direction_keeps_its_place() {
    let mut held = HeldInputs::new();
    held.set_pov1(0, -1);
    held.press(button(Button::Fire));
    held.set_pov1(0, -1);
    assert_eq!(held.last(), Some(button(Button::Fire)));
}

#[test]
fn a_snapshot_orders_keys_by_code_then_pov1_directions() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    // 715 is mode 1, always held; 289 Fire, 290 A.
    let held = HeldInputs::from_snapshot(&map, &[290, 715, 289], (0, 0));
    assert_eq!(
        held.last(),
        Some(button(Button::ButtonA)),
        "the highest code"
    );
    let mut held = HeldInputs::from_snapshot(&map, &[290, 715, 289], (1, -1));
    assert_eq!(held.last(), Some(POV1_RIGHT), "POV 1 last, up before right");
    held.set_pov1(0, -1);
    assert_eq!(held.last(), Some(POV1_UP));
    held.set_pov1(0, 0);
    assert_eq!(held.last(), Some(button(Button::ButtonA)));
    held.release(button(Button::ButtonA));
    assert_eq!(held.last(), Some(button(Button::Fire)));
    assert_eq!(
        HeldInputs::from_snapshot(&map, &[715, 999], (0, 0)).last(),
        None,
        "mode keys and unknown codes"
    );
}

#[test]
fn clearing_drops_everything() {
    let mut held =
        HeldInputs::from_snapshot(&InputMap::for_device(DeviceType::X52Pro), &[289], (0, 1));
    held.clear();
    assert_eq!(held, HeldInputs::new());
}

#[test]
fn held_inputs_answer_whether_an_input_is_held() {
    let mut held = HeldInputs::new();
    held.press(button(Button::Fire));
    held.press(button(Button::Mode2));
    held.set_pov1(1, 0);
    assert!(held.is_held(button(Button::Fire)));
    assert!(held.is_held(POV1_RIGHT));
    assert!(!held.is_held(POV1_UP));
    assert!(!held.is_held(button(Button::Mode2)), "the mode selector");
    held.release(button(Button::Fire));
    assert!(!held.is_held(button(Button::Fire)));
}

#[test]
fn button_numbers_follow_the_ascending_key_codes() {
    // The prototype's numbers on the Pro (art/prototype, "BUTTONS"): 288 is 1, 726 is 39.
    let pro = InputMap::for_device(DeviceType::X52Pro);
    for (index, (code, _)) in pro.buttons().enumerate() {
        assert_eq!(pro.button_number(code), Some(index + 1), "code {code}");
    }
    assert_eq!(pro.button_number(289), Some(2), "Fire");
    assert_eq!(pro.button_number(718), Some(31), "i");
    assert_eq!(pro.button_number(304), None);
    let x52 = InputMap::for_device(DeviceType::X52);
    let numbers: BTreeSet<usize> = x52
        .buttons()
        .filter_map(|(code, _)| x52.button_number(code))
        .collect();
    assert_eq!(numbers, (1..=34).collect(), "every X52 button once");
    assert_eq!(x52.button_number(717), Some(30), "the X52's clutch");
}

#[test]
fn code_of_inverts_the_map() {
    for device in DeviceType::ALL {
        let map = InputMap::for_device(device);
        for (code, button) in map.buttons() {
            assert_eq!(map.code_of(button), Some(code), "{device} {button:?}");
        }
    }
    let x52 = InputMap::for_device(DeviceType::X52);
    assert_eq!(x52.code_of(Button::Clutch), Some(717));
    assert_eq!(x52.code_of(Button::Select), None);
}

#[test]
fn the_ministick_is_only_on_the_pro() {
    assert!(InputMap::for_device(DeviceType::X52Pro).has_axis(Axis::MinistickX));
    assert!(!InputMap::for_device(DeviceType::X52).has_axis(Axis::MinistickY));
    assert!(InputMap::for_device(DeviceType::X52).has_axis(Axis::Slider));
}

#[test]
fn an_axis_fraction_is_clamped_to_its_range() {
    let range = AxisRange { min: 0, max: 1023 };
    assert!((range.fraction(0) - 0.0).abs() < f64::EPSILON);
    assert!((range.fraction(1023) - 1.0).abs() < f64::EPSILON);
    assert!((range.fraction(2000) - 1.0).abs() < f64::EPSILON);
    assert!((range.fraction(-5) - 0.0).abs() < f64::EPSILON);
    let centred = AxisRange { min: -8, max: 8 };
    assert!((centred.fraction(0) - 0.5).abs() < f64::EPSILON);
    let empty = AxisRange { min: 5, max: 5 };
    assert!((empty.fraction(5) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn axis_values_change_only_the_axes_of_the_snapshot() {
    let range = AxisRange { min: 0, max: 255 };
    let mut values = AxisValues::new();
    values.set_value(Axis::Throttle, 10);
    assert_eq!(values.get(Axis::Throttle), None, "no snapshot yet");
    values.set_snapshot([(Axis::Throttle, AxisState { range, value: 3 })]);
    values.set_value(Axis::Throttle, 200);
    values.set_value(Axis::Slider, 9);
    assert_eq!(
        values.get(Axis::Throttle),
        Some(AxisState { range, value: 200 })
    );
    assert_eq!(values.get(Axis::Slider), None);
    values.clear();
    assert_eq!(values, AxisValues::new());
}

#[test]
fn a_centring_axis_rests_in_the_middle() {
    let ministick = AxisRange { min: 0, max: 15 };
    assert_eq!(ministick.centre(), 8);
    assert!((ministick.centred_fraction(8) - 0.5).abs() < f64::EPSILON);
    assert!((ministick.centred_fraction(0) - 0.0).abs() < f64::EPSILON);
    assert!((ministick.centred_fraction(15) - 1.0).abs() < f64::EPSILON);
    assert!((ministick.centred_fraction(4) - 0.25).abs() < f64::EPSILON);
    let stick = AxisRange { min: 0, max: 1023 };
    assert_eq!(stick.centre(), 512);
    assert!((stick.centred_fraction(512) - 0.5).abs() < f64::EPSILON);
    assert!((stick.centred_fraction(2000) - 1.0).abs() < f64::EPSILON);
    let empty = AxisRange { min: 3, max: 3 };
    assert!((empty.centred_fraction(3) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn centring_survives_extreme_and_tiny_ranges() {
    let widest = AxisRange {
        min: i32::MIN,
        max: i32::MAX,
    };
    assert_eq!(widest.centre(), 0);
    assert!((widest.centred_fraction(i32::MAX) - 1.0).abs() < f64::EPSILON);
    assert!((widest.centred_fraction(i32::MIN) - 0.0).abs() < f64::EPSILON);
    let two = AxisRange { min: 0, max: 1 };
    assert_eq!(two.centre(), 1);
    assert!(
        (two.centred_fraction(1) - 1.0).abs() < f64::EPSILON,
        "max is the end"
    );
    assert!((two.centred_fraction(0) - 0.0).abs() < f64::EPSILON);
    let negative = AxisRange { min: -8, max: 7 };
    assert_eq!(negative.centre(), 0);
}
