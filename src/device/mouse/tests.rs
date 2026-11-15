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

/// `EV_KEY`, `EV_REL`, `EV_SYN`.
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_SYN: u16 = 0x00;
/// `REL_X`, `REL_Y`, `REL_WHEEL`, `REL_WHEEL_HI_RES`.
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL: u16 = 0x08;
const REL_WHEEL_HI_RES: u16 = 0x0b;
/// `BTN_LEFT`, `BTN_RIGHT`.
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;

fn raw(event_type: u16, code: u16, value: i32) -> RawEvent {
    RawEvent {
        event_type,
        code,
        value,
    }
}

const SYN: RawEvent = RawEvent {
    event_type: EV_SYN,
    code: 0,
    value: 0,
};

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

#[test]
fn inside_the_deadzone_there_is_no_speed() {
    let tuning = Tuning::default();
    for offset in [0.0, 0.5, 1.0, -1.0] {
        assert!(close(tuning.speed(offset), 0.0), "offset {offset}");
    }
    assert!(!tuning.moves(1));
    assert!(!tuning.moves(-1));
    assert!(tuning.moves(2));
    assert!(tuning.moves(-2));
    let none = Tuning {
        deadzone: 0.0,
        ..Tuning::default()
    };
    assert!(none.moves(1), "no deadzone: one step moves");
}

#[test]
fn speed_follows_gain_curve_and_sign() {
    let tuning = Tuning::default();
    // One step past the deadzone: the gain alone.
    assert!(close(tuning.speed(2.0), 0.6));
    assert!(close(tuning.speed(-2.0), -0.6));
    assert!(close(tuning.speed(7.0), 0.6 * 6.0_f64.powf(1.6)));
    let linear = Tuning {
        gain: 2.0,
        curve: 1.0,
        deadzone: 0.0,
        ..Tuning::default()
    };
    assert!(close(linear.speed(3.0), 6.0));
    assert!(close(linear.speed(-7.0), -14.0));
    assert!(
        tuning.speed(7.0) > tuning.speed(3.0) * 3.0,
        "steeper than linear"
    );
}

#[test]
fn slow_speeds_accumulate_and_the_carry_resets_at_the_centre() {
    let tuning = Tuning::default();
    let mut motion = AxisMotion::default();
    // 0.6 px per tick: 0, 1, 0, 1, 1, 0, 1, 0, 1, 1 (6 px in 10 ticks, give or take rounding).
    let moved: i32 = (0..10).map(|_| motion.step(2, &tuning)).sum();
    assert!((5..=6).contains(&moved), "moved {moved} px");
    let mut motion = AxisMotion::default();
    assert_eq!(motion.step(2, &tuning), 0, "0.6 px carried");
    assert_eq!(motion.step(0, &tuning), 0, "centred");
    assert_eq!(
        motion.step(2, &tuning),
        0,
        "the carry was dropped: 0.6 again"
    );
    assert_eq!(motion.step(2, &tuning), 1, "1.2 px");
    assert_eq!(motion.step(-2, &tuning), 0, "0.2 − 0.6 = −0.4");
    assert_eq!(motion.step(-2, &tuning), -1, "−1.0");
}

#[test]
fn inside_the_deadzone_off_centre_the_carry_stays() {
    let tuning = Tuning::default();
    let mut motion = AxisMotion::default();
    assert_eq!(motion.step(2, &tuning), 0);
    assert_eq!(motion.step(1, &tuning), 0, "deadzone: no speed, 0.6 kept");
    assert_eq!(motion.step(2, &tuning), 1, "1.2 px");
    motion.reset();
    assert_eq!(motion.step(2, &tuning), 0, "reset drops it");
}

#[test]
fn the_mouse_buttons_map_to_left_right_and_the_wheel() {
    let tuning = Tuning::default();
    assert_eq!(
        pointer_event(Button::MouseButton, true, &tuning),
        Some(PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true
        })
    );
    assert_eq!(
        pointer_event(Button::ScrollPress, false, &tuning),
        Some(PointerEvent::Button {
            button: PointerButton::Right,
            pressed: false
        })
    );
    assert_eq!(
        pointer_event(Button::ScrollUp, true, &tuning),
        Some(PointerEvent::Wheel { notches: 1 })
    );
    assert_eq!(
        pointer_event(Button::ScrollDown, true, &tuning),
        Some(PointerEvent::Wheel { notches: -1 })
    );
    assert_eq!(
        pointer_event(Button::ScrollUp, false, &tuning),
        None,
        "notch release"
    );
    assert_eq!(pointer_event(Button::ScrollDown, false, &tuning), None);
    assert_eq!(pointer_event(Button::Fire, true, &tuning), None);
    assert_eq!(pointer_event(Button::Clutch, true, &tuning), None);
}

#[test]
fn invert_wheel_flips_the_notches_only() {
    let inverted = Tuning {
        wheel: WheelDirection::Inverted,
        ..Tuning::default()
    };
    assert_eq!(
        pointer_event(Button::ScrollUp, true, &inverted),
        Some(PointerEvent::Wheel { notches: -1 })
    );
    assert_eq!(
        pointer_event(Button::ScrollDown, true, &inverted),
        Some(PointerEvent::Wheel { notches: 1 })
    );
    assert_eq!(
        pointer_event(Button::MouseButton, true, &inverted),
        Some(PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true
        })
    );
}

#[test]
fn only_the_four_mouse_controls_are_mouse_buttons() {
    let mouse: Vec<Button> = [
        Button::MouseButton,
        Button::ScrollPress,
        Button::ScrollUp,
        Button::ScrollDown,
        Button::Fire,
        Button::Clutch,
        Button::Mode1,
    ]
    .into_iter()
    .filter(|button| is_mouse_button(*button))
    .collect();
    assert_eq!(
        mouse,
        [
            Button::MouseButton,
            Button::ScrollPress,
            Button::ScrollUp,
            Button::ScrollDown
        ]
    );
}

#[test]
fn motion_reports_only_the_axes_that_moved() {
    assert_eq!(
        encode(&[PointerEvent::Motion { dx: 3, dy: -2 }]),
        [raw(EV_REL, REL_X, 3), raw(EV_REL, REL_Y, -2), SYN]
    );
    assert_eq!(
        encode(&[PointerEvent::Motion { dx: 0, dy: 4 }]),
        [raw(EV_REL, REL_Y, 4), SYN]
    );
    assert_eq!(
        encode(&[PointerEvent::Motion { dx: -1, dy: 0 }]),
        [raw(EV_REL, REL_X, -1), SYN]
    );
    assert!(encode(&[PointerEvent::Motion { dx: 0, dy: 0 }]).is_empty());
    assert!(encode(&[]).is_empty());
}

#[test]
fn buttons_and_wheel_encode_and_every_batch_ends_with_syn_report() {
    assert_eq!(
        encode(&[PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true
        }]),
        [raw(EV_KEY, BTN_LEFT, 1), SYN]
    );
    assert_eq!(
        encode(&[
            PointerEvent::Button {
                button: PointerButton::Left,
                pressed: false
            },
            PointerEvent::Button {
                button: PointerButton::Right,
                pressed: false
            }
        ]),
        [raw(EV_KEY, BTN_LEFT, 0), raw(EV_KEY, BTN_RIGHT, 0), SYN]
    );
    assert_eq!(
        encode(&[PointerEvent::Wheel { notches: 1 }]),
        [
            raw(EV_REL, REL_WHEEL, 1),
            raw(EV_REL, REL_WHEEL_HI_RES, 120),
            SYN
        ]
    );
    assert_eq!(
        encode(&[PointerEvent::Wheel { notches: -1 }]),
        [
            raw(EV_REL, REL_WHEEL, -1),
            raw(EV_REL, REL_WHEEL_HI_RES, -120),
            SYN
        ]
    );
    assert_eq!(RawEvent::SYN_REPORT, SYN);
}

#[test]
fn the_defaults_are_the_spike_values() {
    assert_eq!(
        Tuning::default(),
        Tuning {
            gain: 0.6,
            curve: 1.6,
            deadzone: 1.0,
            wheel: WheelDirection::Natural,
        }
    );
}
