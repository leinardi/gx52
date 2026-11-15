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

//! The virtual-mouse loop (docs/architecture.md, "Virtual-mouse loop"): its own thread, fed by
//! one FIFO channel from the main thread (the switch and the tuning) and from the evdev reader
//! (the ministick and the mouse buttons), so both are seen in a single order.
//!
//! The uinput pointer exists only while the switch is on and a joystick is present. The loop acts
//! on what arrives after [`MouseMessage::Enable`]: nothing that arrived before it, or after
//! [`MouseMessage::Disable`], is emitted, and a button is released only when the loop emitted its
//! press, so no button stays held. On `Gone`, `Disable` and every failure both buttons are
//! released before the pointer is dropped. A failed creation or write drops the pointer, reports
//! the failure once and retries the creation every [`RETRY_INTERVAL`].
//!
//! While the pointer exists and a ministick axis is outside the deadzone, motion ticks fall every
//! [`TICK`] on a monotonic grid that incoming messages do not move; missed ticks are skipped, not
//! caught up. Otherwise the loop blocks until a message, or the next retry, arrives.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, KeyCode, RelativeAxisCode};
use x52::device::DeviceType;
use x52::input::Button;

use super::monotonic::Monotonic;
use super::mouse::{
    AxisMotion, MINISTICK_CENTRE, PointerButton, PointerEvent, RawEvent, Tuning, encode,
    pointer_event,
};

/// One motion step every 10 ms (100 Hz), independent of how often the device reports (S9).
pub(crate) const TICK: Duration = Duration::from_millis(10);

/// How often a failed pointer is created again while the switch is on and a joystick is present:
/// the period of the Preferences access check, so a fixed `/dev/uinput` brings the pointer back
/// within it, without a replug or a toggle.
pub(crate) const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// The name of the uinput device, as `/proc/bus/input/devices` and the desktop show it.
pub(crate) const VIRTUAL_MOUSE_NAME: &str = "GX52 virtual mouse";

/// A ministick axis.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the evdev reader, the next M7 step, sends them")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StickAxis {
    /// Left-right: `REL_X`.
    Horizontal,
    /// Forward-back: `REL_Y`.
    Vertical,
}

/// A message to the loop, from the main thread or the evdev reader.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the evdev reader, the next M7 step, sends the reader's messages"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MouseMessage {
    /// The switch is on, with this tuning (main thread).
    Enable(Tuning),
    /// The tuning changed while the switch is on (main thread).
    Tune(Tuning),
    /// The switch is off (main thread).
    Disable,
    /// Stop the thread (main thread).
    Shutdown,
    /// The reader opened the evdev node of a joystick of this type.
    Present(DeviceType),
    /// A ministick axis reports `value`, `0..=15` (the reader, also once per axis on open).
    Stick {
        /// The axis.
        axis: StickAxis,
        /// The raw value.
        value: i32,
    },
    /// One of the four mouse buttons was pressed or released (the reader).
    Button {
        /// The button.
        button: Button,
        /// Pressed, or released.
        pressed: bool,
    },
    /// The reader closed the node: the joystick went away or is no longer selected.
    Gone,
}

/// What failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Creating the uinput device.
    Create,
    /// Writing to it.
    Write,
}

/// A failure of the virtual mouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MouseFailure {
    /// What failed.
    pub(crate) stage: Stage,
    /// The kind of the I/O error, `PermissionDenied` without the uinput rule.
    pub(crate) kind: io::ErrorKind,
    /// The I/O error, as text.
    pub(crate) message: String,
}

impl MouseFailure {
    fn new(stage: Stage, error: &io::Error) -> Self {
        Self {
            stage,
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for MouseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = &self.message;
        match self.stage {
            Stage::Create => write!(formatter, "cannot create the virtual mouse: {message}"),
            Stage::Write => write!(formatter, "cannot write to the virtual mouse: {message}"),
        }
    }
}

/// The state of the virtual mouse, reported to the main thread on every change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MouseStatus {
    /// The switch is off.
    Off,
    /// The switch is on, but no joystick is present.
    NoJoystick,
    /// The pointer exists.
    Running,
    /// The pointer failed; it is created again every [`RETRY_INTERVAL`].
    Failed(MouseFailure),
}

/// A uinput pointer: one batch of events per call.
pub(crate) trait Pointer {
    /// Writes `events`, a batch [`encode`] made, ending with `SYN_REPORT`.
    ///
    /// # Errors
    ///
    /// The I/O error of the write.
    fn emit(&mut self, events: &[RawEvent]) -> io::Result<()>;
}

/// Creates the uinput pointer; the loop is generic over it so the tests can use fakes.
pub(crate) trait PointerFactory {
    /// A created pointer; dropping it removes the device.
    type Pointer: Pointer;

    /// Creates the pointer.
    ///
    /// # Errors
    ///
    /// The I/O error of the creation, `PermissionDenied` without the uinput rule.
    fn create(&mut self) -> io::Result<Self::Pointer>;
}

/// Whether the loop goes on after a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    /// Wait for the next message.
    Continue,
    /// Stop the thread.
    Stop,
}

/// The loop's state machine; [`MouseLoop::run`] drives it with the channel and a monotonic clock,
/// the tests with `now` by hand.
pub(crate) struct MouseLoop<F: PointerFactory> {
    factory: F,
    /// The tuning while the switch is on.
    tuning: Option<Tuning>,
    /// Whether the reader has a joystick's node open.
    present: bool,
    /// The ministick's raw values, as last reported.
    stick_x: i32,
    stick_y: i32,
    motion_x: AxisMotion,
    motion_y: AxisMotion,
    pointer: Option<F::Pointer>,
    /// The buttons whose press the loop emitted and has not released.
    held: BTreeSet<PointerButton>,
    /// The last failure, until the pointer exists again or the failure no longer applies.
    failure: Option<MouseFailure>,
    /// The next motion tick, while one is due.
    next_tick: Option<Duration>,
    /// The next creation attempt after a failure.
    retry_at: Option<Duration>,
    /// The status last reported.
    reported: MouseStatus,
}

impl<F: PointerFactory> MouseLoop<F> {
    /// A loop with the switch off, no joystick and no pointer.
    pub(crate) fn new(factory: F) -> Self {
        Self {
            factory,
            tuning: None,
            present: false,
            stick_x: MINISTICK_CENTRE,
            stick_y: MINISTICK_CENTRE,
            motion_x: AxisMotion::default(),
            motion_y: AxisMotion::default(),
            pointer: None,
            held: BTreeSet::new(),
            failure: None,
            next_tick: None,
            retry_at: None,
            reported: MouseStatus::Off,
        }
    }

    /// Runs until [`MouseMessage::Shutdown`] or until every sender is gone, reporting every status
    /// change to `events`.
    pub(crate) fn run<M: Monotonic, E: From<MouseStatus>>(
        mut self,
        messages: &Receiver<MouseMessage>,
        monotonic: &M,
        events: &async_channel::Sender<E>,
    ) {
        loop {
            let received = match self.deadline() {
                None => messages.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(at) => messages.recv_timeout(at.saturating_sub(monotonic.elapsed())),
            };
            let now = monotonic.elapsed();
            match received {
                Ok(message) => {
                    if self.on_message(message, now) == Flow::Stop {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.release_and_drop();
                    return;
                }
            }
            self.on_time(now);
            if let Some(status) = self.take_status() {
                // An unbounded channel only fails once the main thread has stopped listening.
                let _ = events.try_send(status.into());
            }
        }
    }

    /// Applies `message`, received at monotonic time `now`.
    pub(crate) fn on_message(&mut self, message: MouseMessage, now: Duration) -> Flow {
        match message {
            MouseMessage::Enable(tuning) => {
                self.tuning = Some(tuning);
                if self.present && self.pointer.is_none() {
                    self.create(now);
                }
            }
            MouseMessage::Tune(tuning) => {
                if self.tuning.is_some() {
                    self.tuning = Some(tuning);
                    self.schedule(now);
                }
            }
            MouseMessage::Disable => {
                self.release_and_drop();
                self.tuning = None;
                self.failure = None;
                self.retry_at = None;
            }
            MouseMessage::Shutdown => {
                self.release_and_drop();
                return Flow::Stop;
            }
            MouseMessage::Present(device) => {
                tracing::debug!("Virtual mouse: an {device} is present");
                self.present = true;
                self.stick_x = MINISTICK_CENTRE;
                self.stick_y = MINISTICK_CENTRE;
                if self.tuning.is_some() && self.pointer.is_none() {
                    self.create(now);
                }
            }
            MouseMessage::Stick { axis, value } => {
                let (stick, motion) = match axis {
                    StickAxis::Horizontal => (&mut self.stick_x, &mut self.motion_x),
                    StickAxis::Vertical => (&mut self.stick_y, &mut self.motion_y),
                };
                *stick = value;
                if value == MINISTICK_CENTRE {
                    motion.reset();
                }
                self.schedule(now);
            }
            MouseMessage::Button { button, pressed } => self.on_button(button, pressed, now),
            MouseMessage::Gone => {
                self.release_and_drop();
                self.present = false;
                self.stick_x = MINISTICK_CENTRE;
                self.stick_y = MINISTICK_CENTRE;
                self.failure = None;
                self.retry_at = None;
            }
        }
        Flow::Continue
    }

    /// Runs the retry and the motion tick that are due at monotonic time `now`.
    pub(crate) fn on_time(&mut self, now: Duration) {
        if let Some(at) = self.retry_at
            && at <= now
        {
            self.retry_at = None;
            if self.tuning.is_some() && self.present && self.pointer.is_none() {
                self.create(now);
            }
        }
        if let Some(due) = self.next_tick
            && due <= now
        {
            self.tick(due, now);
        }
    }

    /// When the loop has to run next without a message: the next tick or retry, if any.
    pub(crate) fn deadline(&self) -> Option<Duration> {
        match (self.next_tick, self.retry_at) {
            (Some(tick), Some(retry)) => Some(tick.min(retry)),
            (tick, retry) => tick.or(retry),
        }
    }

    /// The current status.
    pub(crate) fn status(&self) -> MouseStatus {
        if self.tuning.is_none() {
            MouseStatus::Off
        } else if !self.present {
            MouseStatus::NoJoystick
        } else if self.pointer.is_some() {
            MouseStatus::Running
        } else if let Some(failure) = &self.failure {
            MouseStatus::Failed(failure.clone())
        } else {
            // Not reached: a present joystick with the switch on has a pointer or a failure.
            MouseStatus::NoJoystick
        }
    }

    /// The status, when it changed since the last call.
    pub(crate) fn take_status(&mut self) -> Option<MouseStatus> {
        let status = self.status();
        (status != self.reported).then(|| {
            self.reported = status.clone();
            status
        })
    }

    fn on_button(&mut self, button: Button, pressed: bool, now: Duration) {
        let (Some(tuning), Some(_)) = (self.tuning, &self.pointer) else {
            return;
        };
        let Some(event) = pointer_event(button, pressed, &tuning) else {
            return;
        };
        if let PointerEvent::Button { button, pressed } = event {
            if pressed {
                self.held.insert(button);
            } else if !self.held.remove(&button) {
                // Its press arrived before the pointer existed: nothing to release.
                return;
            }
        }
        self.emit(&[event], now);
    }

    /// One motion step for the tick due at `due`, run at `now`; schedules the next one on the
    /// grid while an axis is outside the deadzone, skipping the ticks already missed.
    fn tick(&mut self, due: Duration, now: Duration) {
        self.next_tick = None;
        let Some(tuning) = self.tuning else {
            return;
        };
        let dx = self.motion_x.step(self.stick_x - MINISTICK_CENTRE, &tuning);
        let dy = self.motion_y.step(self.stick_y - MINISTICK_CENTRE, &tuning);
        if !self.emit(&[PointerEvent::Motion { dx, dy }], now) {
            return;
        }
        if self.moving() {
            self.next_tick = Some(next_on_grid(due, now));
        }
    }

    /// Starts the ticks at `now` when the pointer moves and none is due.
    fn schedule(&mut self, now: Duration) {
        if self.next_tick.is_none() && self.moving() {
            self.next_tick = Some(now.saturating_add(TICK));
        }
    }

    /// Whether the pointer exists and an axis is outside the deadzone.
    fn moving(&self) -> bool {
        let Some(tuning) = self.tuning else {
            return false;
        };
        self.pointer.is_some()
            && (tuning.moves(self.stick_x - MINISTICK_CENTRE)
                || tuning.moves(self.stick_y - MINISTICK_CENTRE))
    }

    /// Creates the pointer; on failure, drops it, records the failure and arms the retry.
    fn create(&mut self, now: Duration) {
        match self.factory.create() {
            Ok(pointer) => {
                self.pointer = Some(pointer);
                self.failure = None;
                self.retry_at = None;
                self.held.clear();
                self.motion_x.reset();
                self.motion_y.reset();
                self.schedule(now);
            }
            Err(error) => self.fail(MouseFailure::new(Stage::Create, &error), now),
        }
    }

    /// Writes `events`; false when there was no pointer or the write failed (and was handled).
    fn emit(&mut self, events: &[PointerEvent], now: Duration) -> bool {
        let raw = encode(events);
        if raw.is_empty() {
            return true;
        }
        let Some(pointer) = &mut self.pointer else {
            return false;
        };
        match pointer.emit(&raw) {
            Ok(()) => true,
            Err(error) => {
                self.fail(MouseFailure::new(Stage::Write, &error), now);
                false
            }
        }
    }

    /// Releases the buttons and drops the pointer, records `failure` and retries in
    /// [`RETRY_INTERVAL`].
    fn fail(&mut self, failure: MouseFailure, now: Duration) {
        self.release_and_drop();
        self.failure = Some(failure);
        self.retry_at = Some(now.saturating_add(RETRY_INTERVAL));
    }

    /// Releases both buttons (best effort: the pointer may be what failed), then drops the
    /// pointer, which removes the device.
    fn release_and_drop(&mut self) {
        if let Some(mut pointer) = self.pointer.take() {
            let release = PointerButton::ALL.map(|button| PointerEvent::Button {
                button,
                pressed: false,
            });
            if let Err(error) = pointer.emit(&encode(&release)) {
                tracing::debug!("Cannot release the virtual mouse's buttons: {error}");
            }
        }
        self.held.clear();
        self.next_tick = None;
        self.motion_x.reset();
        self.motion_y.reset();
    }
}

/// The first tick after `now` on the grid of `due` + n × [`TICK`].
fn next_on_grid(due: Duration, now: Duration) -> Duration {
    let mut next = due.saturating_add(TICK);
    if next <= now {
        let behind = now.saturating_sub(due).as_nanos() / TICK.as_nanos();
        let skipped = u32::try_from(behind.saturating_add(1)).unwrap_or(u32::MAX);
        next = due.saturating_add(TICK.saturating_mul(skipped));
    }
    next
}

/// Creates the uinput device "GX52 virtual mouse": `BTN_LEFT`, `BTN_RIGHT`, `REL_X`, `REL_Y`,
/// `REL_WHEEL` and `REL_WHEEL_HI_RES`.
///
/// # Errors
///
/// The I/O error of opening `/dev/uinput` or creating the device.
pub(crate) fn create_virtual_mouse() -> io::Result<VirtualDevice> {
    let mut buttons = AttributeSet::<KeyCode>::new();
    buttons.insert(KeyCode::BTN_LEFT);
    buttons.insert(KeyCode::BTN_RIGHT);
    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    for axis in [
        RelativeAxisCode::REL_X,
        RelativeAxisCode::REL_Y,
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_WHEEL_HI_RES,
    ] {
        axes.insert(axis);
    }
    VirtualDevice::builder()?
        .name(VIRTUAL_MOUSE_NAME)
        .with_keys(&buttons)?
        .with_relative_axes(&axes)?
        .build()
}

/// The [`PointerFactory`] of the real system: a uinput device.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UinputFactory;

impl PointerFactory for UinputFactory {
    type Pointer = UinputPointer;

    fn create(&mut self) -> io::Result<UinputPointer> {
        Ok(UinputPointer(create_virtual_mouse()?))
    }
}

/// A uinput pointer; dropping it removes the device.
pub(crate) struct UinputPointer(VirtualDevice);

impl Pointer for UinputPointer {
    fn emit(&mut self, events: &[RawEvent]) -> io::Result<()> {
        // evdev's emit ends every batch with the SYN_REPORT itself.
        let body = events
            .strip_suffix(&[RawEvent::SYN_REPORT])
            .unwrap_or(events);
        let events: Vec<evdev::InputEvent> = body
            .iter()
            .map(|event| evdev::InputEvent::new(event.event_type, event.code, event.value))
            .collect();
        self.0.emit(&events)
    }
}

#[cfg(test)]
mod tests;
