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

//! The writer driven message by message and wake by wake, over a fake opener whose handles are
//! [`RecordingTransport`]s, a [`FakeClock`] and a [`FakeMonotonic`].

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use jiff::{SignedDuration, Timestamp};
use x52::clock::FakeClock;
use x52::command::{Brightness, Command, OffsetClock};
use x52::device::{BiColourLed, DeviceType, LedColour};
use x52::transport::{InjectedFailure, RecordingTransport};

use super::*;
use crate::device::id::Attachment;
use crate::device::monotonic::FakeMonotonic;

/// Every transfer sent, across every opened handle.
type Log = Arc<Mutex<Vec<ControlTransfer>>>;

/// A handle that records into the shared log what its [`RecordingTransport`] accepted.
struct LoggedTransport {
    inner: RecordingTransport,
    log: Log,
}

impl Transport for LoggedTransport {
    type Error = InjectedFailure;

    fn control_out(&mut self, transfer: ControlTransfer) -> Result<(), Self::Error> {
        self.inner.control_out(transfer)?;
        self.log.lock().unwrap().push(transfer);
        Ok(())
    }
}

/// Opens with the scripted outcomes in order, then with a recording handle every time.
struct FakeOpener {
    script: VecDeque<Result<RecordingTransport, OpenError>>,
    log: Log,
    opens: Arc<Mutex<Vec<DeviceId>>>,
}

impl Opener for FakeOpener {
    type Transport = LoggedTransport;

    fn open(&mut self, device: &DeviceId) -> Result<LoggedTransport, OpenError> {
        self.opens.lock().unwrap().push(device.clone());
        let inner = self
            .script
            .pop_front()
            .unwrap_or_else(|| Ok(RecordingTransport::new()))?;
        Ok(LoggedTransport {
            inner,
            log: Arc::clone(&self.log),
        })
    }
}

/// The fake clock, shared with the test.
struct SharedClock(Arc<FakeClock>);

impl Clock for SharedClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

struct Harness {
    writer: Writer<FakeOpener, SharedClock, FakeMonotonic>,
    clock: Arc<FakeClock>,
    monotonic: FakeMonotonic,
    log: Log,
    opens: Arc<Mutex<Vec<DeviceId>>>,
    events: async_channel::Receiver<WriterEvent>,
}

const START: &str = "2026-11-03T12:00:30Z";

fn utc() -> TimeZone {
    TimeZone::UTC
}

fn rome() -> TimeZone {
    TimeZone::get("Europe/Rome").unwrap()
}

impl Harness {
    fn new(script: Vec<Result<RecordingTransport, OpenError>>) -> Self {
        Self::at(START, utc, script)
    }

    fn at(
        now: &str,
        local_zone: fn() -> TimeZone,
        script: Vec<Result<RecordingTransport, OpenError>>,
    ) -> Self {
        let clock = Arc::new(FakeClock::new(now.parse().unwrap()));
        let monotonic = FakeMonotonic::default();
        let log = Log::default();
        let opens = Arc::default();
        let (sender, events) = async_channel::unbounded();
        let opener = FakeOpener {
            script: script.into(),
            log: Arc::clone(&log),
            opens: Arc::clone(&opens),
        };
        let writer = Writer::new(
            opener,
            SharedClock(Arc::clone(&clock)),
            monotonic.clone(),
            local_zone,
            sender,
        );
        Self {
            writer,
            clock,
            monotonic,
            log,
            opens,
            events,
        }
    }

    /// Handles `messages`, then wakes once.
    fn send(&mut self, messages: impl IntoIterator<Item = WriterMessage>) {
        for message in messages {
            assert!(self.writer.handle(message).is_continue());
        }
        self.writer.wake();
    }

    /// The transfers sent since the last call.
    fn sent(&self) -> Vec<ControlTransfer> {
        std::mem::take(&mut *self.log.lock().unwrap())
    }

    fn opens(&self) -> usize {
        self.opens.lock().unwrap().len()
    }

    fn access_events(&self) -> Vec<AccessState> {
        std::iter::from_fn(|| self.events.try_recv().ok())
            .filter_map(|event| match event {
                WriterEvent::Access(access) => Some(access),
                WriterEvent::Error(_) => None,
            })
            .collect()
    }

    /// The transfers of every command of `state` on an X52 Pro, at the fake clock's time.
    fn full_replay(&self, state: &DeviceState) -> Vec<ControlTransfer> {
        transfers(
            &state
                .resolve(
                    DeviceType::X52Pro,
                    self.clock.now(),
                    &(self.writer.local_zone)(),
                )
                .unwrap(),
        )
    }

    /// The transfers of the commands of `state` in `slots`, in replay order.
    fn of_slots(&self, state: &DeviceState, slots: &[CommandSlot]) -> Vec<ControlTransfer> {
        let commands: Vec<Command> = state
            .resolve(
                DeviceType::X52Pro,
                self.clock.now(),
                &(self.writer.local_zone)(),
            )
            .unwrap()
            .into_iter()
            .filter(|command| slots.contains(&command.slot()))
            .collect();
        assert_eq!(commands.len(), slots.len(), "{slots:?}");
        transfers(&commands)
    }
}

fn transfers(commands: &[Command]) -> Vec<ControlTransfer> {
    commands.iter().flat_map(Command::encode).collect()
}

fn device(generation: u64, address: u8) -> DeviceId {
    DeviceId {
        device_type: DeviceType::X52Pro,
        sysfs_path: PathBuf::from("/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4"),
        attachment: Attachment { bus: 1, address },
        generation: Generation(generation),
    }
}

fn pro_default() -> DeviceState {
    DeviceState::default_for(DeviceType::X52Pro)
}

/// A harness whose device is open and has had the full replay of the default state.
fn connected() -> Harness {
    let mut harness = Harness::new(Vec::new());
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.sent();
    harness
}

#[test]
fn connect_sends_the_exact_full_replay() {
    let mut harness = Harness::new(Vec::new());

    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);

    let expected = harness.full_replay(&pro_default());
    assert_eq!(harness.sent(), expected);
    // Brightness first (docs/protocol.md, "Power-on state").
    assert_eq!(
        (expected[0].index(), expected[1].index()),
        (0xb1, 0xb2),
        "{expected:?}"
    );
    assert_eq!(harness.access_events(), [AccessState::Ready]);
}

#[test]
fn nothing_is_sent_before_a_state_or_a_selection() {
    let mut harness = Harness::new(Vec::new());
    harness.send([WriterMessage::Selected(device(1, 5))]);
    assert_eq!(harness.sent(), []);

    let mut harness = Harness::new(Vec::new());
    harness.send([WriterMessage::SetState(pro_default())]);
    assert_eq!(harness.sent(), []);
    assert_eq!(harness.opens(), 0);
}

#[test]
fn a_profile_switch_sends_only_the_differing_commands() {
    let mut harness = connected();
    let mut profile = pro_default();
    profile.mfd_brightness = Brightness::new(32).unwrap();
    profile
        .leds
        .set_colour(BiColourLed::ButtonE, LedColour::Red);
    profile.shift = true;

    harness.send([WriterMessage::SetState(profile.clone())]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(
            &profile,
            &[
                CommandSlot::MfdBrightness,
                CommandSlot::BiColourLed(BiColourLed::ButtonE),
                CommandSlot::Shift,
            ]
        )
    );
}

#[test]
fn a_burst_of_brightness_changes_queued_before_a_drain_is_one_transfer() {
    let mut harness = connected();
    let (sender, receiver) = mpsc::channel();
    let mut state = pro_default();
    for level in 0..50 {
        state.led_brightness = Brightness::new(level).unwrap();
        sender.send(WriterMessage::SetState(state.clone())).unwrap();
    }

    assert!(harness.writer.step(&receiver).is_continue());

    assert_eq!(
        harness
            .sent()
            .iter()
            .map(|transfer| (transfer.index(), transfer.value()))
            .collect::<Vec<_>>(),
        [(0xb2, 49)]
    );
}

#[test]
fn shutdown_and_a_closed_channel_stop_the_loop() {
    let mut harness = connected();
    let (sender, receiver) = mpsc::channel();
    sender.send(WriterMessage::Shutdown).unwrap();
    assert!(harness.writer.step(&receiver).is_break());
    drop(sender);
    assert!(harness.writer.step(&receiver).is_break());
}

#[test]
fn a_minute_rollover_resends_clock_1_only() {
    let mut harness = connected();
    harness.clock.set("2026-11-03T12:00:59Z".parse().unwrap());
    harness.send([]);
    assert_eq!(harness.sent(), [], "nothing changed within the minute");

    harness.clock.set("2026-11-03T12:01:00Z".parse().unwrap());
    harness.send([]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(&pro_default(), &[CommandSlot::Clock1])
    );
}

#[test]
fn midnight_resends_clock_1_and_the_date() {
    let mut harness = Harness::at("2026-11-03T23:59:30Z", utc, Vec::new());
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.sent();

    harness.clock.set("2026-11-04T00:00:00Z".parse().unwrap());
    harness.send([]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(&pro_default(), &[CommandSlot::Clock1, CommandSlot::Date])
    );
}

#[test]
fn a_dst_transition_resends_clock_1_and_the_affected_offsets() {
    // Europe/Rome goes from UTC+1 to UTC+2 at 01:00 UTC on 29 March 2026. Clock 2 follows the
    // same zone, so its offset from clock 1 stays 0; clock 3 is fixed, so its offset moves.
    let mut harness = Harness::at("2026-03-29T00:59:30Z", rome, Vec::new());
    let mut state = pro_default();
    state.clock2.source = x52::clock::ZoneSource::iana("Europe/Rome").unwrap();
    harness.send([
        WriterMessage::SetState(state.clone()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.sent();
    assert_eq!(
        harness.writer.timeout(),
        Duration::from_secs(5),
        "capped by the wake interval"
    );

    harness.clock.set("2026-03-29T01:00:00Z".parse().unwrap());
    harness.send([]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(
            &state,
            &[
                CommandSlot::Clock1,
                CommandSlot::ClockOffset(OffsetClock::Clock3)
            ]
        )
    );
}

#[test]
fn the_timeout_is_the_next_clock_change_capped_at_the_wake_interval() {
    let harness = connected();
    assert_eq!(harness.writer.timeout(), WAKE_INTERVAL);

    harness.clock.set("2026-11-03T12:00:58Z".parse().unwrap());
    assert_eq!(harness.writer.timeout(), Duration::from_secs(2));
}

#[test]
fn a_forward_jump_resends_the_clocks_at_the_next_wake() {
    let mut harness = connected();
    // A resume from a 10-minute suspend: the wall clock jumped, the monotonic one did not.
    harness.clock.advance(SignedDuration::from_mins(10));
    assert!(harness.writer.timeout() <= WAKE_INTERVAL);

    harness.send([]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(&pro_default(), &[CommandSlot::Clock1])
    );
}

#[test]
fn a_backward_jump_resends_the_clocks_at_the_next_wake() {
    let mut harness = connected();
    harness.clock.advance(SignedDuration::from_hours(-1));
    assert!(harness.writer.timeout() <= WAKE_INTERVAL);

    harness.send([]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(&pro_default(), &[CommandSlot::Clock1])
    );
}

#[test]
fn a_failed_transfer_drops_the_handle_and_the_reopen_a_second_later_replays_fully() {
    let replay_len = connected().full_replay(&pro_default()).len();
    // The first handle fails on the first transfer after the replay.
    let mut harness = Harness::new(vec![Ok(RecordingTransport::failing_at(replay_len))]);
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.sent();
    let mut state = pro_default();
    state.mfd_brightness = Brightness::new(0).unwrap();

    harness.send([WriterMessage::SetState(state.clone())]);
    assert_eq!(harness.sent(), []);
    assert_eq!(harness.writer.timeout(), RETRY_INTERVAL);
    assert!(matches!(
        harness.events.try_recv(),
        Ok(WriterEvent::Access(AccessState::Ready))
    ));
    assert!(matches!(
        harness.events.try_recv(),
        Ok(WriterEvent::Error(WriterError::Transfer { .. }))
    ));

    harness.monotonic.advance(Duration::from_millis(999));
    harness.send([]);
    assert_eq!(harness.opens(), 1, "not before the retry interval");

    harness.monotonic.advance(Duration::from_millis(1));
    harness.send([]);
    assert_eq!(harness.opens(), 2);
    assert_eq!(harness.sent(), harness.full_replay(&state));
}

#[test]
fn a_new_generation_at_the_same_path_reopens_and_replays_fully() {
    let mut harness = connected();

    // Unplugged and plugged into the same port: same sysfs path, new address, new generation.
    harness.send([WriterMessage::Selected(device(2, 6))]);

    assert_eq!(harness.opens(), 2);
    assert_eq!(harness.opens.lock().unwrap()[1], device(2, 6));
    assert_eq!(harness.sent(), harness.full_replay(&pro_default()));
}

#[test]
fn the_same_selection_again_does_not_reopen() {
    let mut harness = connected();
    harness.send([WriterMessage::Selected(device(1, 5))]);
    assert_eq!(harness.opens(), 1);
    assert_eq!(harness.sent(), []);
}

#[test]
fn a_denied_permission_is_retried_every_second_without_any_message() {
    let mut harness = Harness::new(vec![Err(OpenError::PermissionDenied)]);
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    assert_eq!(harness.access_events(), [AccessState::NoPermission]);
    assert_eq!(harness.sent(), []);
    assert_eq!(harness.writer.timeout(), RETRY_INTERVAL);

    // The ACL comes back with `udevadm trigger`: no hotplug event, no Selected, no Retry.
    harness.monotonic.advance(RETRY_INTERVAL);
    harness.send([]);

    assert_eq!(harness.opens(), 2);
    assert_eq!(harness.access_events(), [AccessState::Ready]);
    assert_eq!(harness.sent(), harness.full_replay(&pro_default()));
}

#[test]
fn retry_opens_at_once_after_a_denied_permission() {
    let mut harness = Harness::new(vec![
        Err(OpenError::PermissionDenied),
        Err(OpenError::PermissionDenied),
    ]);
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.send([WriterMessage::Retry]);
    assert_eq!(harness.opens(), 2);
    assert_eq!(
        harness.access_events(),
        [AccessState::NoPermission],
        "reported once"
    );

    harness.send([WriterMessage::Retry]);

    assert_eq!(harness.opens(), 3);
    assert_eq!(harness.access_events(), [AccessState::Ready]);
    assert_eq!(harness.sent(), harness.full_replay(&pro_default()));
}

#[test]
fn selected_opens_at_once_after_a_denied_permission() {
    let mut harness = Harness::new(vec![Err(OpenError::PermissionDenied)]);
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);

    harness.send([WriterMessage::Selected(device(1, 5))]);

    assert_eq!(harness.opens(), 2);
    assert_eq!(harness.sent(), harness.full_replay(&pro_default()));
}

#[test]
fn another_open_failure_is_reported_once_and_retried() {
    let mut harness = Harness::new(vec![
        Err(OpenError::Failed(io::Error::other("busy"))),
        Err(OpenError::Failed(io::Error::other("busy"))),
    ]);
    harness.send([
        WriterMessage::SetState(pro_default()),
        WriterMessage::Selected(device(1, 5)),
    ]);
    harness.monotonic.advance(RETRY_INTERVAL);
    harness.send([]);
    let errors = std::iter::from_fn(|| harness.events.try_recv().ok())
        .filter(|event| matches!(event, WriterEvent::Error(WriterError::Open(_))))
        .count();
    assert_eq!(errors, 1);

    harness.monotonic.advance(RETRY_INTERVAL);
    harness.send([]);

    assert_eq!(harness.opens(), 3);
    assert_eq!(harness.sent(), harness.full_replay(&pro_default()));
}

#[test]
fn gone_with_a_stale_generation_is_ignored() {
    let mut harness = connected();
    harness.send([WriterMessage::Selected(device(2, 6))]);
    harness.sent();
    harness.access_events();

    harness.send([WriterMessage::Gone(Generation(1))]);
    let mut state = pro_default();
    state.blink = true;
    harness.send([WriterMessage::SetState(state.clone())]);

    assert_eq!(
        harness.sent(),
        harness.of_slots(&state, &[CommandSlot::Blink])
    );
    assert_eq!(harness.access_events(), []);
}

#[test]
fn gone_for_the_selection_closes_the_device() {
    let mut harness = connected();
    harness.access_events();

    harness.send([WriterMessage::Gone(Generation(1))]);
    let mut state = pro_default();
    state.blink = true;
    harness.send([WriterMessage::SetState(state)]);

    assert_eq!(harness.sent(), []);
    assert_eq!(harness.access_events(), [AccessState::NoDevice]);
    assert_eq!(harness.writer.timeout(), WAKE_INTERVAL);
}
