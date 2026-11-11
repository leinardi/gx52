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

//! The evdev input map of each device type, the debounce of the mode selector and the set of held
//! inputs.
//!
//! The codes are evdev's key and absolute-axis codes as plain numbers (docs/protocol.md, "evdev
//! input map (X52 Pro)" and "X52 (non-Pro)"); the reader turns events into them. The mode
//! debounce works on decoded modes and evdev's event timestamps, never on the wall clock.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::device::DeviceType;

/// A button of the stick or the throttle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    /// The trigger, first stage.
    TriggerStage1,
    /// The trigger, second stage (the first stage stays held).
    TriggerStage2,
    /// Fire, under the flip cover.
    Fire,
    /// The A button.
    ButtonA,
    /// The B button.
    ButtonB,
    /// The C button.
    ButtonC,
    /// The pinkie switch.
    Pinkie,
    /// The D button.
    ButtonD,
    /// The E button.
    ButtonE,
    /// Toggle switch 1 up.
    T1,
    /// Toggle switch 1 down.
    T2,
    /// Toggle switch 2 up.
    T3,
    /// Toggle switch 2 down.
    T4,
    /// Toggle switch 3 up.
    T5,
    /// Toggle switch 3 down.
    T6,
    /// The mouse button next to the ministick.
    MouseButton,
    /// The throttle's scroll wheel, one notch down.
    ScrollDown,
    /// The throttle's scroll wheel, one notch up.
    ScrollUp,
    /// The throttle's scroll wheel pressed.
    ScrollPress,
    /// POV 2 (the top-left hat on the stick head) up.
    Pov2Up,
    /// POV 2 right.
    Pov2Right,
    /// POV 2 down.
    Pov2Down,
    /// POV 2 left.
    Pov2Left,
    /// The throttle hat up.
    ThrottleHatUp,
    /// The throttle hat right.
    ThrottleHatRight,
    /// The throttle hat down.
    ThrottleHatDown,
    /// The throttle hat left.
    ThrottleHatLeft,
    /// The mode selector in position 1 (held while there).
    Mode1,
    /// The mode selector in position 2.
    Mode2,
    /// The mode selector in position 3.
    Mode3,
    /// The i (clutch) button.
    Clutch,
    /// Function: the left MFD wheel pressed.
    Function,
    /// Start/Stop.
    StartStop,
    /// Reset.
    Reset,
    /// The left MFD wheel up: Page Up.
    PageUp,
    /// The left MFD wheel down: Page Down.
    PageDown,
    /// The right MFD wheel up.
    Up,
    /// The right MFD wheel down.
    Down,
    /// Select: the right MFD wheel pressed.
    Select,
}

/// An axis of the stick or the throttle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    /// The stick, left to right (`ABS_X`).
    StickX,
    /// The stick, forward to back (`ABS_Y`).
    StickY,
    /// The stick twist (`ABS_RZ`).
    Twist,
    /// The throttle lever (`ABS_Z`).
    Throttle,
    /// The rotary next to the i button (`ABS_RX`).
    RotaryI,
    /// The rotary next to the E button (`ABS_RY`).
    RotaryE,
    /// The slider on the throttle (`ABS_THROTTLE`).
    Slider,
    /// The ministick, left to right (`ABS_MISC`).
    MinistickX,
    /// The ministick, top to bottom (code 41).
    MinistickY,
    /// POV 1 (the lower hat on the stick), left to right (`ABS_HAT0X`).
    Pov1X,
    /// POV 1, up to down (`ABS_HAT0Y`).
    Pov1Y,
}

/// A direction of POV 1, the hat that is a pair of axes rather than buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Pov1Direction {
    /// Up (`ABS_HAT0Y` −1).
    Up,
    /// Right (`ABS_HAT0X` +1).
    Right,
    /// Down (`ABS_HAT0Y` +1).
    Down,
    /// Left (`ABS_HAT0X` −1).
    Left,
}

impl Pov1Direction {
    /// Every direction, in snapshot order.
    pub const ALL: [Self; 4] = [Self::Up, Self::Right, Self::Down, Self::Left];

    /// Whether the hat at `x`, `y` points this way (diagonals point two ways).
    #[must_use]
    pub const fn is_held(self, x: i32, y: i32) -> bool {
        match self {
            Self::Up => y < 0,
            Self::Right => x > 0,
            Self::Down => y > 0,
            Self::Left => x < 0,
        }
    }
}

/// A control that can be held: a button or a direction of POV 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Input {
    /// A button.
    Button(Button),
    /// A direction of POV 1.
    Pov1(Pov1Direction),
}

/// The inputs held now, in the order they were pressed: the source of the "last button" MFD line
/// (docs/profiles.md, "MFD lines").
///
/// The mode selector positions are never held here: one of them is always held, so they would
/// hide every other button.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldInputs {
    /// Oldest first.
    held: Vec<Input>,
}

impl HeldInputs {
    /// Nothing held.
    #[must_use]
    pub const fn new() -> Self {
        Self { held: Vec::new() }
    }

    /// The inputs of a snapshot taken when a node opens, which says what is held but not since
    /// when: the `keys` (evdev codes, as `EVIOCGKEY` gives them) in ascending code order, then the
    /// POV 1 directions of the hat at `pov1` in [`Pov1Direction::ALL`] order. So [`Self::last`] is
    /// the POV 1 direction if there is one, else the button of the highest key code.
    #[must_use]
    pub fn from_snapshot(map: &InputMap, keys: &[u16], pov1: (i32, i32)) -> Self {
        let mut codes = keys.to_vec();
        codes.sort_unstable();
        let mut held = Self::new();
        for button in codes.into_iter().filter_map(|code| map.button(code)) {
            held.press(Input::Button(button));
        }
        held.set_pov1(pov1.0, pov1.1);
        held
    }

    /// `input` was pressed. A mode selector position, or an input already held (an evdev repeat,
    /// a snapshot), changes nothing: the input keeps its place.
    pub fn press(&mut self, input: Input) {
        if !is_mode(input) && !self.held.contains(&input) {
            self.held.push(input);
        }
    }

    /// `input` was released.
    pub fn release(&mut self, input: Input) {
        self.held.retain(|held| *held != input);
    }

    /// POV 1 is at `x`, `y` now: the directions that left are released and the ones that joined
    /// are pressed; a direction held before and after keeps its place.
    pub fn set_pov1(&mut self, x: i32, y: i32) {
        for direction in Pov1Direction::ALL {
            let input = Input::Pov1(direction);
            if direction.is_held(x, y) {
                self.press(input);
            } else {
                self.release(input);
            }
        }
    }

    /// Nothing is held any more: the device or its node went away.
    pub fn clear(&mut self) {
        self.held.clear();
    }

    /// The most recently pressed input that is still held.
    #[must_use]
    pub fn last(&self) -> Option<Input> {
        self.held.last().copied()
    }

    /// Whether `input` is held now. A mode selector position never is.
    #[must_use]
    pub fn is_held(&self, input: Input) -> bool {
        self.held.contains(&input)
    }
}

/// Whether `input` is a position of the mode selector.
const fn is_mode(input: Input) -> bool {
    matches!(
        input,
        Input::Button(Button::Mode1 | Button::Mode2 | Button::Mode3)
    )
}

/// A position of the mode selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mode {
    /// Mode 1.
    One,
    /// Mode 2.
    Two,
    /// Mode 3.
    Three,
}

impl Mode {
    /// Every mode.
    pub const ALL: [Self; 3] = [Self::One, Self::Two, Self::Three];

    const fn bit(self) -> u8 {
        match self {
            Self::One => 0b001,
            Self::Two => 0b010,
            Self::Three => 0b100,
        }
    }
}

/// The X52 Pro's buttons (S3, every control pressed by hand on 2026-11-02).
const PRO_BUTTONS: [(u16, Button); 39] = [
    (288, Button::TriggerStage1),
    (289, Button::Fire),
    (290, Button::ButtonA),
    (291, Button::ButtonB),
    (292, Button::ButtonC),
    (293, Button::Pinkie),
    (294, Button::ButtonD),
    (295, Button::ButtonE),
    (296, Button::T1),
    (297, Button::T2),
    (298, Button::T3),
    (299, Button::T4),
    (300, Button::T5),
    (301, Button::T6),
    (302, Button::TriggerStage2),
    (303, Button::MouseButton),
    (704, Button::ScrollDown),
    (705, Button::ScrollUp),
    (706, Button::ScrollPress),
    (707, Button::Pov2Up),
    (708, Button::Pov2Right),
    (709, Button::Pov2Down),
    (710, Button::Pov2Left),
    (711, Button::ThrottleHatUp),
    (712, Button::ThrottleHatRight),
    (713, Button::ThrottleHatDown),
    (714, Button::ThrottleHatLeft),
    (715, Button::Mode1),
    (716, Button::Mode2),
    (717, Button::Mode3),
    (718, Button::Clutch),
    (719, Button::Function),
    (720, Button::StartStop),
    (721, Button::Reset),
    (722, Button::PageUp),
    (723, Button::PageDown),
    (724, Button::Up),
    (725, Button::Down),
    (726, Button::Select),
];

/// The X52's buttons: GX52 0.7's candidate map, not verified on a device. It has no Page Up,
/// Page Down, Up, Down or Select.
const X52_BUTTONS: [(u16, Button); 34] = [
    (288, Button::TriggerStage1),
    (289, Button::Fire),
    (290, Button::ButtonA),
    (291, Button::ButtonB),
    (292, Button::ButtonC),
    (293, Button::Pinkie),
    (294, Button::ButtonD),
    (295, Button::ButtonE),
    (296, Button::T1),
    (297, Button::T2),
    (298, Button::T3),
    (299, Button::T4),
    (300, Button::T5),
    (301, Button::T6),
    (302, Button::TriggerStage2),
    (303, Button::Pov2Up),
    (704, Button::Pov2Right),
    (705, Button::Pov2Down),
    (706, Button::Pov2Left),
    (707, Button::ThrottleHatUp),
    (708, Button::ThrottleHatRight),
    (709, Button::ThrottleHatDown),
    (710, Button::ThrottleHatLeft),
    (711, Button::Mode1),
    (712, Button::Mode2),
    (713, Button::Mode3),
    (714, Button::Function),
    (715, Button::StartStop),
    (716, Button::Reset),
    (717, Button::Clutch),
    (718, Button::MouseButton),
    (719, Button::ScrollPress),
    (720, Button::ScrollDown),
    (721, Button::ScrollUp),
];

/// The X52 Pro's axes, by evdev absolute-axis code.
const PRO_AXES: [(u16, Axis); 11] = [
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
];

/// The X52's axes: assumed as on the Pro, without the ministick, whose codes are not known.
const X52_AXES: [(u16, Axis); 9] = [
    (0x00, Axis::StickX),
    (0x01, Axis::StickY),
    (0x02, Axis::Throttle),
    (0x03, Axis::RotaryI),
    (0x04, Axis::RotaryE),
    (0x05, Axis::Twist),
    (0x06, Axis::Slider),
    (0x10, Axis::Pov1X),
    (0x11, Axis::Pov1Y),
];

/// The evdev codes of one device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMap {
    buttons: &'static [(u16, Button)],
    axes: &'static [(u16, Axis)],
    verified: bool,
}

impl InputMap {
    /// The map of `device`.
    #[must_use]
    pub const fn for_device(device: DeviceType) -> Self {
        match device {
            DeviceType::X52Pro => Self {
                buttons: &PRO_BUTTONS,
                axes: &PRO_AXES,
                verified: true,
            },
            DeviceType::X52 => Self {
                buttons: &X52_BUTTONS,
                axes: &X52_AXES,
                verified: false,
            },
        }
    }

    /// Whether the map was checked on a device; the X52's is GX52 0.7's unverified candidate.
    #[must_use]
    pub const fn is_verified(&self) -> bool {
        self.verified
    }

    /// The button of an `EV_KEY` code, or `None` for a code the device does not have.
    #[must_use]
    pub fn button(&self, code: u16) -> Option<Button> {
        lookup(self.buttons, code)
    }

    /// The axis of an `EV_ABS` code, or `None` for a code the device does not have.
    #[must_use]
    pub fn axis(&self, code: u16) -> Option<Axis> {
        lookup(self.axes, code)
    }

    /// The mode an `EV_KEY` code selects, or `None` for any other code.
    #[must_use]
    pub fn mode_for(&self, code: u16) -> Option<Mode> {
        match self.button(code)? {
            Button::Mode1 => Some(Mode::One),
            Button::Mode2 => Some(Mode::Two),
            Button::Mode3 => Some(Mode::Three),
            Button::TriggerStage1
            | Button::TriggerStage2
            | Button::Fire
            | Button::ButtonA
            | Button::ButtonB
            | Button::ButtonC
            | Button::Pinkie
            | Button::ButtonD
            | Button::ButtonE
            | Button::T1
            | Button::T2
            | Button::T3
            | Button::T4
            | Button::T5
            | Button::T6
            | Button::MouseButton
            | Button::ScrollDown
            | Button::ScrollUp
            | Button::ScrollPress
            | Button::Pov2Up
            | Button::Pov2Right
            | Button::Pov2Down
            | Button::Pov2Left
            | Button::ThrottleHatUp
            | Button::ThrottleHatRight
            | Button::ThrottleHatDown
            | Button::ThrottleHatLeft
            | Button::Clutch
            | Button::Function
            | Button::StartStop
            | Button::Reset
            | Button::PageUp
            | Button::PageDown
            | Button::Up
            | Button::Down
            | Button::Select => None,
        }
    }

    /// The `EV_KEY` code of `button`, or `None` when the device does not have it.
    #[must_use]
    pub fn code_of(&self, button: Button) -> Option<u16> {
        self.buttons
            .iter()
            .find(|(_, known)| *known == button)
            .map(|(code, _)| *code)
    }

    /// The number games show for the button of `code`, from 1: its place among the device's key
    /// codes in ascending order, the order joydev and SDL number buttons in. The mode selector
    /// positions count, since games see them too. `None` for a code the device does not have.
    #[must_use]
    pub fn button_number(&self, code: u16) -> Option<usize> {
        self.button(code)?;
        Some(
            self.buttons
                .iter()
                .filter(|(known, _)| *known <= code)
                .count(),
        )
    }

    /// Whether the device has `axis`.
    #[must_use]
    pub fn has_axis(&self, axis: Axis) -> bool {
        self.axes.iter().any(|(_, known)| *known == axis)
    }

    /// Every button code and its button.
    pub fn buttons(&self) -> impl Iterator<Item = (u16, Button)> {
        self.buttons.iter().copied()
    }

    /// Every axis code and its axis.
    pub fn axes(&self) -> impl Iterator<Item = (u16, Axis)> {
        self.axes.iter().copied()
    }
}

fn lookup<T: Copy>(table: &[(u16, T)], code: u16) -> Option<T> {
    table
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, item)| *item)
}

/// The values an absolute axis reports, as the device's absinfo (`EVIOCGABS`) gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisRange {
    /// The lowest value.
    pub min: i32,
    /// The highest value.
    pub max: i32,
}

impl AxisRange {
    /// Where `value` lies in the range, from `0.0` at [`Self::min`] to `1.0` at [`Self::max`],
    /// clamped; `0.0` for an empty or inverted range.
    #[must_use]
    pub fn fraction(self, value: i32) -> f64 {
        if self.max <= self.min {
            return 0.0;
        }
        let span = f64::from(self.max) - f64::from(self.min);
        ((f64::from(value) - f64::from(self.min)) / span).clamp(0.0, 1.0)
    }
}

/// The range and the current value of an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxisState {
    /// What the axis can report.
    pub range: AxisRange,
    /// What it reports now.
    pub value: i32,
}

/// The axes of the open node and their values, for the input tester.
///
/// A snapshot fills it when the node opens; later values change only the axes it has, so a value
/// is never shown against a range that was not read from the device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxisValues {
    axes: BTreeMap<Axis, AxisState>,
}

impl AxisValues {
    /// No axis known.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            axes: BTreeMap::new(),
        }
    }

    /// Replaces every axis with those of a snapshot.
    pub fn set_snapshot(&mut self, axes: impl IntoIterator<Item = (Axis, AxisState)>) {
        self.axes = axes.into_iter().collect();
    }

    /// `axis` reports `value` now; ignored for an axis the snapshot did not have.
    pub fn set_value(&mut self, axis: Axis, value: i32) {
        if let Some(state) = self.axes.get_mut(&axis) {
            state.value = value;
        }
    }

    /// The range and value of `axis`, if known.
    #[must_use]
    pub fn get(&self, axis: Axis) -> Option<AxisState> {
        self.axes.get(&axis).copied()
    }

    /// Nothing is known any more: the device or its node went away.
    pub fn clear(&mut self) {
        self.axes.clear();
    }
}

/// A set of modes: the mode buttons held at one time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModeSet(u8);

impl ModeSet {
    /// No mode held.
    pub const EMPTY: Self = Self(0);

    /// Whether `mode` is in the set.
    #[must_use]
    pub const fn contains(self, mode: Mode) -> bool {
        self.0 & mode.bit() != 0
    }

    /// The set with `mode` added.
    #[must_use]
    pub const fn with(self, mode: Mode) -> Self {
        Self(self.0 | mode.bit())
    }

    /// The set with `mode` removed.
    #[must_use]
    pub const fn without(self, mode: Mode) -> Self {
        Self(self.0 & !mode.bit())
    }

    /// Whether no mode is held.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The one mode held, or `None` when none or several are.
    #[must_use]
    pub fn sole(self) -> Option<Mode> {
        let mut held = Mode::ALL.into_iter().filter(|mode| self.contains(*mode));
        let first = held.next()?;
        held.next().is_none().then_some(first)
    }
}

impl FromIterator<Mode> for ModeSet {
    fn from_iter<I: IntoIterator<Item = Mode>>(modes: I) -> Self {
        modes.into_iter().fold(Self::EMPTY, Self::with)
    }
}

/// How long one mode must be held alone before it is applied.
pub const SETTLE: Duration = Duration::from_millis(200);

/// The debounce of the mode selector (docs/protocol.md, "Mode selector").
///
/// A mode becomes active only after it has been the one mode held, alone, for [`SETTLE`]. A gap
/// with no mode held keeps the active mode, and a mode passed through for less than [`SETTLE`]
/// (2, between 3 and 1) is never applied. Times are evdev's event timestamps, as a [`Duration`]
/// since any fixed origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeDebouncer {
    held: ModeSet,
    active: Option<Mode>,
    /// The mode held alone that is not active yet, and since when.
    pending: Option<(Mode, Duration)>,
}

impl ModeDebouncer {
    /// A debouncer starting from the mode buttons held in the `EVIOCGKEY` snapshot taken at `at`.
    ///
    /// One mode held is active at once: a snapshot is a settled position, not a bounce. With
    /// several held no mode is active until one of them is held alone for [`SETTLE`]; with none
    /// held, until the first mode is. The 200 ms of a later hold count from that event's own
    /// timestamp, so the debounce does not use `at`: it is part of the reader's API, the time of
    /// the snapshot. Events queued before the snapshot are harmless when replayed, since pressing
    /// a mode already held or releasing one that is not changes nothing.
    #[must_use]
    pub fn new(held: ModeSet, at: Duration) -> Self {
        let _ = at;
        Self {
            held,
            active: held.sole(),
            pending: None,
        }
    }

    /// The active mode, if one is.
    #[must_use]
    pub const fn active(&self) -> Option<Mode> {
        self.active
    }

    /// When the pending mode is applied if nothing changes before: the reader's poll timeout.
    #[must_use]
    pub fn deadline(&self) -> Option<Duration> {
        self.pending.map(|(_, since)| since.saturating_add(SETTLE))
    }

    /// Applies the pending mode if it has been held alone for [`SETTLE`] at `at`, and returns it.
    pub fn advance_to(&mut self, at: Duration) -> Option<Mode> {
        let (mode, _) = self.pending?;
        if at < self.deadline()? {
            return None;
        }
        self.pending = None;
        self.active = Some(mode);
        Some(mode)
    }

    /// A mode button pressed or released at `at`; returns the mode applied by this call, if any.
    ///
    /// A pending mode whose time has come by `at` is applied first, so a release exactly
    /// [`SETTLE`] after the press still applies it.
    pub fn on_event(&mut self, mode: Mode, pressed: bool, at: Duration) -> Option<Mode> {
        let applied = self.advance_to(at);
        let before = self.held;
        self.held = if pressed {
            before.with(mode)
        } else {
            before.without(mode)
        };
        if self.held != before {
            self.pending = match self.held.sole() {
                Some(sole) if Some(sole) != self.active => Some((sole, at)),
                Some(_) | None => None,
            };
        }
        applied
    }
}

#[cfg(test)]
mod tests;
