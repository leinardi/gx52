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

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use super::*;
use crate::device::monotonic::FakeMonotonic;
use crate::device::mouse::WheelDirection;

/// `EV_KEY`, `EV_REL`.
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
/// `BTN_LEFT`, `BTN_RIGHT`, `REL_X`, `REL_WHEEL`.
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const REL_X: u16 = 0x00;
const REL_WHEEL: u16 = 0x08;

/// What the fake pointers saw, shared with the test.
#[derive(Debug, Default)]
struct Log {
    /// Pointers created.
    created: usize,
    /// Pointers alive now.
    alive: usize,
    /// Every batch written successfully, in order.
    batches: Vec<Vec<RawEvent>>,
    /// Batches whose write failed.
    failed_writes: usize,
    /// The next creations fail with this error kind.
    fail_create: Option<io::ErrorKind>,
    /// The next writes fail.
    fail_write: bool,
}

#[derive(Debug, Clone, Default)]
struct Shared(Arc<Mutex<Log>>);

impl Shared {
    fn with<T>(&self, body: impl FnOnce(&mut Log) -> T) -> T {
        body(&mut self.0.lock().unwrap())
    }

    fn created(&self) -> usize {
        self.with(|log| log.created)
    }

    fn alive(&self) -> usize {
        self.with(|log| log.alive)
    }

    fn batches(&self) -> Vec<Vec<RawEvent>> {
        self.with(|log| log.batches.clone())
    }

    fn last(&self) -> Option<Vec<RawEvent>> {
        self.with(|log| log.batches.last().cloned())
    }

    fn clear(&self) {
        self.with(|log| log.batches.clear());
    }

    fn fail_create(&self, kind: Option<io::ErrorKind>) {
        self.with(|log| log.fail_create = kind);
    }

    fn fail_write(&self, fail: bool) {
        self.with(|log| log.fail_write = fail);
    }
}

struct FakeFactory(Shared);

struct FakePointer(Shared);

impl PointerFactory for FakeFactory {
    type Pointer = FakePointer;

    fn create(&mut self) -> io::Result<FakePointer> {
        self.0.with(|log| {
            if let Some(kind) = log.fail_create {
                return Err(io::Error::from(kind));
            }
            log.created += 1;
            log.alive += 1;
            Ok(())
        })?;
        Ok(FakePointer(self.0.clone()))
    }
}

impl Pointer for FakePointer {
    fn emit(&mut self, events: &[RawEvent]) -> io::Result<()> {
        self.0.with(|log| {
            if log.fail_write {
                log.failed_writes += 1;
                Err(io::Error::from_raw_os_error(19))
            } else {
                log.batches.push(events.to_vec());
                Ok(())
            }
        })
    }
}

impl Drop for FakePointer {
    fn drop(&mut self) {
        self.0.with(|log| log.alive -= 1);
    }
}

fn mouse() -> (MouseLoop<FakeFactory>, Shared) {
    let shared = Shared::default();
    (MouseLoop::new(FakeFactory(shared.clone())), shared)
}

const fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn send(mouse: &mut MouseLoop<FakeFactory>, message: MouseMessage, at: Duration) {
    assert_eq!(mouse.on_message(message, at), Flow::Continue);
    mouse.on_time(at);
}

fn raw(event_type: u16, code: u16, value: i32) -> RawEvent {
    RawEvent {
        event_type,
        code,
        value,
    }
}

fn left(pressed: bool) -> Vec<RawEvent> {
    vec![
        raw(EV_KEY, BTN_LEFT, i32::from(pressed)),
        RawEvent::SYN_REPORT,
    ]
}

fn release_both() -> Vec<RawEvent> {
    vec![
        raw(EV_KEY, BTN_LEFT, 0),
        raw(EV_KEY, BTN_RIGHT, 0),
        RawEvent::SYN_REPORT,
    ]
}

fn press(button: Button) -> MouseMessage {
    MouseMessage::Button {
        button,
        pressed: true,
    }
}

fn release(button: Button) -> MouseMessage {
    MouseMessage::Button {
        button,
        pressed: false,
    }
}

fn stick_x(value: i32) -> MouseMessage {
    MouseMessage::Stick {
        axis: StickAxis::Horizontal,
        value,
    }
}

/// A tuning that moves at least one pixel on every tick at full deflection.
fn fast() -> Tuning {
    Tuning {
        gain: 3.0,
        curve: 1.0,
        deadzone: 0.0,
        wheel: WheelDirection::Natural,
    }
}

/// The switch on and a joystick present, at time 0.
fn running() -> (MouseLoop<FakeFactory>, Shared) {
    let (mut mouse, shared) = mouse();
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(0));
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    assert_eq!(mouse.take_status(), Some(MouseStatus::Running));
    (mouse, shared)
}

fn motion_batches(shared: &Shared) -> usize {
    shared
        .batches()
        .iter()
        .filter(|batch| batch.iter().any(|event| event.event_type == EV_REL))
        .count()
}

#[test]
fn the_pointer_exists_only_while_enabled_and_present() {
    let (mut mouse, shared) = mouse();
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    assert_eq!(shared.created(), 0, "switch off");
    assert_eq!(mouse.take_status(), None, "still off");
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(1));
    assert_eq!((shared.created(), shared.alive()), (1, 1));
    assert_eq!(mouse.take_status(), Some(MouseStatus::Running));
    send(&mut mouse, MouseMessage::Gone, ms(2));
    assert_eq!(shared.alive(), 0, "unplugged: the device is removed");
    assert_eq!(
        shared.last(),
        Some(release_both()),
        "released before the drop"
    );
    assert_eq!(mouse.take_status(), Some(MouseStatus::NoJoystick));
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(3));
    assert_eq!(
        (shared.created(), shared.alive()),
        (2, 1),
        "replug: a new one"
    );
    send(&mut mouse, MouseMessage::Disable, ms(4));
    assert_eq!(shared.alive(), 0);
    assert_eq!(shared.last(), Some(release_both()));
    assert_eq!(mouse.take_status(), Some(MouseStatus::Off));
}

#[test]
fn a_button_held_at_disable_ends_released() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::MouseButton), ms(1));
    assert_eq!(shared.last(), Some(left(true)));
    send(&mut mouse, MouseMessage::Disable, ms(2));
    assert_eq!(shared.batches(), [left(true), release_both()]);
    assert_eq!(shared.alive(), 0);
    // The release arriving after Disable emits nothing.
    send(&mut mouse, release(Button::MouseButton), ms(3));
    assert_eq!(shared.batches().len(), 2);
}

#[test]
fn a_button_held_at_unplug_ends_released() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::ScrollPress), ms(1));
    send(&mut mouse, MouseMessage::Gone, ms(2));
    assert_eq!(
        shared.batches(),
        [
            vec![raw(EV_KEY, BTN_RIGHT, 1), RawEvent::SYN_REPORT],
            release_both()
        ]
    );
    assert_eq!(shared.alive(), 0);
}

#[test]
fn clicks_and_wheel_follow_the_tuning() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::ScrollUp), ms(1));
    send(&mut mouse, release(Button::ScrollUp), ms(2));
    send(
        &mut mouse,
        MouseMessage::Tune(Tuning {
            wheel: WheelDirection::Inverted,
            ..Tuning::default()
        }),
        ms(3),
    );
    send(&mut mouse, press(Button::ScrollUp), ms(4));
    send(&mut mouse, press(Button::Fire), ms(5));
    let wheel: Vec<i32> = shared
        .batches()
        .iter()
        .map(|batch| batch[0])
        .filter(|event| event.code == REL_WHEEL)
        .map(|event| event.value)
        .collect();
    assert_eq!(wheel, [1, -1], "a notch, then the inverted one; not Fire");
    assert_eq!(shared.batches().len(), 2);
    assert_eq!(shared.created(), 1, "Tune keeps the pointer");
}

#[test]
fn tune_applies_to_the_running_pointer_at_once() {
    let (mut mouse, shared) = running();
    send(&mut mouse, stick_x(10), ms(0));
    mouse.on_time(ms(10));
    // Default tuning, offset 2: 0.6 px, nothing whole yet.
    assert_eq!(motion_batches(&shared), 0);
    send(&mut mouse, MouseMessage::Tune(fast()), ms(15));
    mouse.on_time(ms(20));
    assert_eq!(
        shared.last(),
        Some(vec![raw(EV_REL, REL_X, 6), RawEvent::SYN_REPORT]),
        "0.6 carried + 3 × 2 = 6.6"
    );
    assert_eq!(shared.created(), 1);
}

#[test]
fn nothing_ticks_while_centred_or_inside_the_deadzone() {
    let (mut mouse, _shared) = running();
    assert_eq!(mouse.deadline(), None, "centred: blocks on recv");
    send(&mut mouse, stick_x(9), ms(1));
    assert_eq!(mouse.deadline(), None, "one step: inside the deadzone");
    send(&mut mouse, stick_x(15), ms(2));
    assert_eq!(mouse.deadline(), Some(ms(12)));
    send(&mut mouse, stick_x(8), ms(5));
    mouse.on_time(ms(12));
    assert_eq!(mouse.deadline(), None, "back at the centre: the ticks stop");
}

#[test]
fn nothing_ticks_without_a_pointer() {
    let (mut mouse, _shared) = mouse();
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    send(&mut mouse, stick_x(15), ms(1));
    assert_eq!(mouse.deadline(), None, "switch off");
    send(&mut mouse, MouseMessage::Enable(fast()), ms(2));
    assert_eq!(
        mouse.deadline(),
        Some(ms(12)),
        "the position held before the switch moves the new pointer"
    );
}

#[test]
fn a_press_and_release_before_enable_emit_nothing() {
    let (mut mouse, shared) = mouse();
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    send(&mut mouse, press(Button::MouseButton), ms(1));
    send(&mut mouse, release(Button::MouseButton), ms(2));
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(3));
    assert_eq!(shared.created(), 1);
    assert!(shared.batches().is_empty());
}

#[test]
fn a_release_whose_press_came_before_enable_emits_nothing() {
    let (mut mouse, shared) = mouse();
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    send(&mut mouse, press(Button::MouseButton), ms(1));
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(2));
    send(&mut mouse, release(Button::MouseButton), ms(3));
    assert!(shared.batches().is_empty());
}

#[test]
fn a_rapid_toggle_emits_nothing_from_between() {
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Disable, ms(1));
    send(&mut mouse, press(Button::MouseButton), ms(2));
    send(&mut mouse, release(Button::MouseButton), ms(3));
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(4));
    assert_eq!(
        shared.batches(),
        [release_both()],
        "only the Disable's release"
    );
    send(&mut mouse, press(Button::MouseButton), ms(5));
    assert_eq!(shared.last(), Some(left(true)), "after the Enable: emitted");
    assert_eq!(shared.created(), 2);
}

#[test]
fn a_press_after_disable_emits_nothing() {
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Disable, ms(1));
    shared.clear();
    send(&mut mouse, press(Button::MouseButton), ms(2));
    send(&mut mouse, press(Button::ScrollUp), ms(3));
    assert!(shared.batches().is_empty());
}

#[test]
fn a_failed_creation_is_reported_once_and_retried_every_second() {
    let (mut mouse, shared) = mouse();
    shared.fail_create(Some(io::ErrorKind::PermissionDenied));
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(0));
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    let Some(MouseStatus::Failed(failure)) = mouse.take_status() else {
        panic!("a failure is reported");
    };
    assert_eq!(failure.stage, Stage::Create);
    assert_eq!(failure.kind, io::ErrorKind::PermissionDenied);
    assert_eq!(mouse.deadline(), Some(RETRY_INTERVAL));
    mouse.on_time(ms(999));
    assert_eq!(mouse.deadline(), Some(RETRY_INTERVAL), "not yet");
    mouse.on_time(ms(1000));
    assert_eq!(
        mouse.take_status(),
        None,
        "the same failure: not reported again"
    );
    assert_eq!(mouse.deadline(), Some(ms(2000)));
    // The rule is installed: the next retry works, without a toggle or a replug.
    shared.fail_create(None);
    mouse.on_time(ms(2000));
    assert_eq!(mouse.take_status(), Some(MouseStatus::Running));
    assert_eq!(shared.created(), 1);
    assert_eq!(mouse.deadline(), None);
}

#[test]
fn no_retry_once_the_failure_no_longer_applies() {
    let (mut mouse, shared) = mouse();
    shared.fail_create(Some(io::ErrorKind::NotFound));
    send(&mut mouse, MouseMessage::Present(DeviceType::X52Pro), ms(0));
    send(&mut mouse, MouseMessage::Enable(Tuning::default()), ms(0));
    send(&mut mouse, MouseMessage::Gone, ms(10));
    assert_eq!(mouse.deadline(), None, "no joystick: nothing to retry");
    assert_eq!(mouse.take_status(), Some(MouseStatus::NoJoystick));
    send(
        &mut mouse,
        MouseMessage::Present(DeviceType::X52Pro),
        ms(20),
    );
    send(&mut mouse, MouseMessage::Disable, ms(30));
    assert_eq!(mouse.deadline(), None, "switch off: nothing to retry");
}

/// Asserts the state after a failed write at `at`: no pointer, one report, the retry armed.
fn assert_write_failure(mouse: &mut MouseLoop<FakeFactory>, shared: &Shared, at: Duration) {
    assert_eq!(shared.alive(), 0, "the pointer is dropped");
    let Some(MouseStatus::Failed(failure)) = mouse.take_status() else {
        panic!("a failure is reported");
    };
    assert_eq!(failure.stage, Stage::Write);
    assert_eq!(mouse.take_status(), None, "reported once");
    assert_eq!(
        mouse.deadline(),
        Some(at + RETRY_INTERVAL),
        "the retry is armed"
    );
    shared.fail_write(false);
    mouse.on_time(at + RETRY_INTERVAL);
    assert_eq!(mouse.take_status(), Some(MouseStatus::Running));
    assert_eq!(shared.alive(), 1);
}

#[test]
fn a_failed_press_drops_the_pointer() {
    let (mut mouse, shared) = running();
    shared.fail_write(true);
    send(&mut mouse, press(Button::MouseButton), ms(5));
    assert_write_failure(&mut mouse, &shared, ms(5));
    // The press was never emitted: its release emits nothing on the new pointer.
    send(&mut mouse, release(Button::MouseButton), ms(2000));
    assert!(shared.batches().is_empty());
}

#[test]
fn a_failed_release_drops_the_pointer() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::MouseButton), ms(1));
    shared.fail_write(true);
    send(&mut mouse, release(Button::MouseButton), ms(5));
    assert_write_failure(&mut mouse, &shared, ms(5));
}

#[test]
fn a_failed_motion_drops_the_pointer() {
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Tune(fast()), ms(0));
    send(&mut mouse, stick_x(15), ms(0));
    shared.fail_write(true);
    mouse.on_time(ms(10));
    assert_write_failure(&mut mouse, &shared, ms(10));
    assert_eq!(
        mouse.deadline(),
        Some(ms(1020)),
        "the stick is still out: the new pointer ticks again"
    );
}

#[test]
fn a_failed_teardown_still_drops_the_pointer() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::MouseButton), ms(1));
    shared.fail_write(true);
    send(&mut mouse, MouseMessage::Gone, ms(2));
    assert_eq!(shared.alive(), 0, "dropped although the release failed");
    assert_eq!(shared.with(|log| log.failed_writes), 1);
    assert_eq!(mouse.take_status(), Some(MouseStatus::NoJoystick));
    assert_eq!(
        mouse.deadline(),
        None,
        "nothing to retry without a joystick"
    );
    // Disable, the other teardown.
    let (mut mouse, shared) = running();
    shared.fail_write(true);
    send(&mut mouse, MouseMessage::Disable, ms(1));
    assert_eq!(shared.alive(), 0);
    assert_eq!(mouse.take_status(), Some(MouseStatus::Off));
    assert_eq!(mouse.deadline(), None);
}

#[test]
fn frequent_axis_reports_do_not_starve_the_ticks() {
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Tune(fast()), ms(0));
    for millis in 0..=100 {
        // The ministick reports every millisecond, alternating between two values.
        send(
            &mut mouse,
            stick_x(14 + i32::from(millis % 2 == 0)),
            ms(millis),
        );
    }
    assert_eq!(motion_batches(&shared), 10, "ticks at 10, 20, …, 100 ms");
}

#[test]
fn a_stall_gives_one_tick_not_a_burst() {
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Tune(fast()), ms(0));
    send(&mut mouse, stick_x(15), ms(0));
    mouse.on_time(ms(10));
    mouse.on_time(ms(20));
    assert_eq!(motion_batches(&shared), 2);
    // The thread is not scheduled for 35 ms: the ticks due at 30, 40 and 50 ms.
    mouse.on_time(ms(55));
    assert_eq!(motion_batches(&shared), 3, "one tick, not three");
    assert_eq!(mouse.deadline(), Some(ms(60)), "back on the 10 ms grid");
}

#[test]
fn the_grid_skips_missed_ticks() {
    assert_eq!(next_on_grid(ms(10), ms(10)), ms(20));
    assert_eq!(next_on_grid(ms(10), ms(19)), ms(20));
    assert_eq!(next_on_grid(ms(10), ms(20)), ms(30));
    assert_eq!(next_on_grid(ms(30), ms(55)), ms(60));
}

#[test]
fn shutdown_releases_and_stops() {
    let (mut mouse, shared) = running();
    send(&mut mouse, press(Button::MouseButton), ms(1));
    assert_eq!(mouse.on_message(MouseMessage::Shutdown, ms(2)), Flow::Stop);
    assert_eq!(shared.last(), Some(release_both()));
    assert_eq!(shared.alive(), 0);
}

#[test]
fn the_thread_reports_each_status_change_once() {
    let (mouse, shared) = mouse();
    shared.fail_create(Some(io::ErrorKind::PermissionDenied));
    let (messages, received) = mpsc::channel();
    let (events, statuses) = async_channel::unbounded::<MouseStatus>();
    let monotonic = FakeMonotonic::default();
    let clock = monotonic.clone();
    let thread = thread::spawn(move || mouse.run(&received, &clock, &events));
    messages
        .send(MouseMessage::Enable(Tuning::default()))
        .unwrap();
    messages
        .send(MouseMessage::Present(DeviceType::X52Pro))
        .unwrap();
    assert_eq!(statuses.recv_blocking(), Ok(MouseStatus::NoJoystick));
    let Ok(MouseStatus::Failed(_)) = statuses.recv_blocking() else {
        panic!("the failure is reported");
    };
    shared.fail_create(None);
    monotonic.advance(RETRY_INTERVAL);
    // Wakes the loop at once instead of after the real second its timeout waits.
    messages
        .send(MouseMessage::Tune(Tuning::default()))
        .unwrap();
    assert_eq!(statuses.recv_blocking(), Ok(MouseStatus::Running));
    messages.send(MouseMessage::Shutdown).unwrap();
    thread.join().unwrap();
    assert_eq!(shared.alive(), 0);
    assert!(statuses.try_recv().is_err(), "nothing else was reported");
}

#[test]
fn failures_read_as_messages() {
    let failure = MouseFailure::new(
        Stage::Create,
        &io::Error::from(io::ErrorKind::PermissionDenied),
    );
    assert_eq!(
        failure.to_string(),
        "cannot create the virtual mouse: permission denied"
    );
}

#[test]
fn both_axes_move_the_pointer() {
    /// `REL_Y`.
    const REL_Y: u16 = 0x01;
    let (mut mouse, shared) = running();
    send(&mut mouse, MouseMessage::Tune(fast()), ms(0));
    send(&mut mouse, stick_x(6), ms(0));
    send(
        &mut mouse,
        MouseMessage::Stick {
            axis: StickAxis::Vertical,
            value: 10,
        },
        ms(0),
    );
    mouse.on_time(ms(10));
    assert_eq!(
        shared.last(),
        Some(vec![
            raw(EV_REL, REL_X, -6),
            raw(EV_REL, REL_Y, 6),
            RawEvent::SYN_REPORT
        ]),
        "left and down"
    );
}
