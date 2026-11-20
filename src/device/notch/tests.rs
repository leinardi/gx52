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

use x52::input::Button;

use super::*;

fn key(button: Button, pressed: bool) -> HeldChange {
    HeldChange::Key { button, pressed }
}

/// Applies `change` to the hold and to `held`, as the application does.
fn step(notch: &mut NotchHold, held: &mut HeldInputs, change: &HeldChange) -> HoldTimer {
    let timer = notch.apply(change, held);
    change.apply(held);
    timer
}

const SCROLL_UP: Input = Input::Button(Button::ScrollUp);
const BUTTON_A: Input = Input::Button(Button::ButtonA);

#[test]
fn a_notch_stays_shown_after_its_release() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    assert_eq!(
        step(&mut notch, &mut held, &key(Button::ScrollUp, true)),
        HoldTimer::Arm
    );
    assert_eq!(
        step(&mut notch, &mut held, &key(Button::ScrollUp, false)),
        HoldTimer::Keep
    );
    assert_eq!(held.last(), None);
    assert_eq!(notch.shown(&held), Some(SCROLL_UP));
    assert_eq!(notch.cancel(), HoldTimer::Cancel);
    assert_eq!(notch.shown(&held), None);
}

#[test]
fn another_notch_rearms_and_replaces_the_hold() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &key(Button::ScrollUp, true));
    step(&mut notch, &mut held, &key(Button::ScrollUp, false));
    assert_eq!(
        step(&mut notch, &mut held, &key(Button::PageDown, true)),
        HoldTimer::Arm
    );
    assert_eq!(notch.shown(&held), Some(Input::Button(Button::PageDown)));
}

#[test]
fn another_press_ends_the_hold_at_once() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &key(Button::ScrollDown, true));
    step(&mut notch, &mut held, &key(Button::ScrollDown, false));
    assert_eq!(
        step(&mut notch, &mut held, &key(Button::ButtonA, true)),
        HoldTimer::Cancel
    );
    assert_eq!(notch.shown(&held), Some(BUTTON_A));
}

#[test]
fn a_held_control_below_the_notch_is_shown_again_when_it_ends() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &key(Button::ButtonA, true));
    step(&mut notch, &mut held, &key(Button::ButtonB, true));
    step(&mut notch, &mut held, &key(Button::Up, true));
    step(&mut notch, &mut held, &key(Button::Up, false));
    assert_eq!(notch.shown(&held), Some(Input::Button(Button::Up)));
    // A release of another control keeps the hold.
    assert_eq!(
        step(&mut notch, &mut held, &key(Button::ButtonB, false)),
        HoldTimer::Keep
    );
    assert_eq!(notch.shown(&held), Some(Input::Button(Button::Up)));
    notch.cancel();
    assert_eq!(notch.shown(&held), Some(BUTTON_A));
}

#[test]
fn a_new_pov1_direction_ends_the_hold_but_a_pov1_release_keeps_it() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &HeldChange::Pov1 { x: 1, y: -1 });
    step(&mut notch, &mut held, &key(Button::ScrollUp, true));
    step(&mut notch, &mut held, &key(Button::ScrollUp, false));
    // Up+Right to Up releases Right.
    assert_eq!(
        step(&mut notch, &mut held, &HeldChange::Pov1 { x: 0, y: -1 }),
        HoldTimer::Keep
    );
    assert_eq!(notch.shown(&held), Some(SCROLL_UP));
    // Up to Up+Left presses Left.
    assert_eq!(
        step(&mut notch, &mut held, &HeldChange::Pov1 { x: -1, y: -1 }),
        HoldTimer::Cancel
    );
    assert_eq!(notch.shown(&held), Some(Input::Pov1(Pov1Direction::Left)));
}

#[test]
fn a_snapshot_ends_the_hold_and_never_arms_it() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &key(Button::ScrollUp, true));
    let mut snapshot = HeldInputs::new();
    snapshot.press(SCROLL_UP);
    assert_eq!(
        step(&mut notch, &mut held, &HeldChange::Snapshot(snapshot)),
        HoldTimer::Cancel
    );
    // The snapshot's scroll notch is shown as held, not held on.
    assert_eq!(notch, NotchHold::default());
    assert_eq!(notch.shown(&held), Some(SCROLL_UP));
}

#[test]
fn a_node_loss_after_a_released_notch_clears_the_line() {
    let mut notch = NotchHold::default();
    let mut held = HeldInputs::new();
    step(&mut notch, &mut held, &key(Button::ScrollUp, true));
    step(&mut notch, &mut held, &key(Button::ScrollUp, false));
    let before = notch.shown(&held);
    // The node goes before the hold's time is up.
    held.clear();
    notch.cancel();
    assert_eq!(before, Some(SCROLL_UP));
    assert_eq!(notch.shown(&held), None);
}
