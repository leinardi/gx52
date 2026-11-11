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

//! The reader over a fake evdev source whose nodes replay scripted events on a fake monotonic
//! clock: an event is returned by the wait whose timeout reaches its timestamp, as poll(2) would.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use x52::device::DeviceType;

use super::*;
use crate::device::id::Attachment;
use crate::device::monotonic::FakeMonotonic;

const USB: &str = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4";
const MODE_1: u16 = 715;
const EV_ABS: u16 = 0x03;

fn device(generation: u64) -> DeviceId {
    DeviceId {
        device_type: DeviceType::X52Pro,
        sysfs_path: PathBuf::from(USB),
        attachment: Attachment { bus: 1, address: 5 },
        generation: Generation(generation),
    }
}

fn entry(number: u32, usb: &str) -> NodeEntry {
    NodeEntry {
        node: PathBuf::from(format!("/dev/input/event{number}")),
        device: PathBuf::from(format!(
            "{usb}/{port}:1.0/0003:06A3:0762.0005/input/input{number}",
            port = usb.rsplit('/').next().unwrap()
        )),
    }
}

fn key(code: u16, pressed: bool, at: Duration) -> InputEvent {
    InputEvent {
        event_type: EV_KEY,
        code,
        value: i32::from(pressed),
        at,
    }
}

/// One scripted happening on a fake node.
#[derive(Debug)]
enum Step {
    /// An event, read by the first wait whose timeout reaches its timestamp.
    Event(InputEvent),
    /// Events read together by one read, at the first one's timestamp.
    Batch(Vec<InputEvent>),
    /// The next read fails with this `errno`.
    Fail(i32),
    /// These messages are sent to the reader during the wait.
    Messages(Vec<ReaderMessage>),
}

struct FakeNode {
    held: Vec<u16>,
    /// The absolute axes' values at the open, by code; every other axis is `0`.
    abs: Vec<(u16, i32)>,
    steps: VecDeque<Step>,
    clock: FakeMonotonic,
    messages: Sender<ReaderMessage>,
    timeouts: Arc<Mutex<Vec<Duration>>>,
    /// Waits in real time too, for the shutdown timing test.
    sleeps: bool,
    /// Once the steps are used up and no mode is pending (a wait of [`POLL_CAP`]), the node sends
    /// `Shutdown`, so a test ends when the reader is idle.
    shuts_down_when_idle: bool,
    queued: Vec<InputEvent>,
    failure: Option<i32>,
}

impl InputNode for FakeNode {
    fn held_keys(&self) -> io::Result<Vec<u16>> {
        Ok(self.held.clone())
    }

    fn abs_info(&self, code: u16) -> io::Result<AxisState> {
        let value = self
            .abs
            .iter()
            .find(|(axis, _)| *axis == code)
            .map_or(0, |(_, value)| *value);
        Ok(AxisState {
            range: fake_range(code),
            value,
        })
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<bool> {
        self.timeouts.lock().unwrap().push(timeout);
        if self.sleeps {
            thread::sleep(timeout);
        }
        let now = self.clock.elapsed();
        match self.steps.front() {
            Some(Step::Event(event)) if event.at <= now + timeout => {
                self.clock.set(event.at.max(now));
                self.queued.push(*event);
                self.steps.pop_front();
                Ok(true)
            }
            Some(Step::Batch(batch))
                if batch.first().is_some_and(|first| first.at <= now + timeout) =>
            {
                self.clock.set(batch[0].at.max(now));
                if let Some(Step::Batch(batch)) = self.steps.pop_front() {
                    self.queued.extend(batch);
                }
                Ok(true)
            }
            Some(Step::Fail(errno)) => {
                self.failure = Some(*errno);
                self.steps.pop_front();
                Ok(true)
            }
            Some(Step::Messages(_)) => {
                if let Some(Step::Messages(messages)) = self.steps.pop_front() {
                    for message in messages {
                        self.messages.send(message).unwrap();
                    }
                }
                self.clock.advance(timeout);
                Ok(false)
            }
            None => {
                if self.shuts_down_when_idle && timeout == POLL_CAP {
                    self.shuts_down_when_idle = false;
                    self.messages.send(ReaderMessage::Shutdown).unwrap();
                }
                self.clock.advance(timeout);
                Ok(false)
            }
            Some(Step::Event(_) | Step::Batch(_)) => {
                self.clock.advance(timeout);
                Ok(false)
            }
        }
    }

    fn read(&mut self) -> io::Result<Vec<InputEvent>> {
        if let Some(errno) = self.failure.take() {
            return Err(io::Error::from_raw_os_error(errno));
        }
        Ok(std::mem::take(&mut self.queued))
    }
}

/// The nodes listed by each discovery (the last list repeats) and the outcome of each open; an
/// open after the scripted ones sends `Shutdown`.
struct FakeSource {
    lists: VecDeque<Vec<NodeEntry>>,
    last: Vec<NodeEntry>,
    opens: VecDeque<Result<FakeNode, io::ErrorKind>>,
    opened: Arc<Mutex<Vec<PathBuf>>>,
    discoveries: Arc<Mutex<usize>>,
    messages: Sender<ReaderMessage>,
}

impl EvdevSource for FakeSource {
    type Node = FakeNode;

    fn nodes(&mut self) -> Vec<NodeEntry> {
        *self.discoveries.lock().unwrap() += 1;
        if let Some(list) = self.lists.pop_front() {
            self.last.clone_from(&list);
        }
        self.last.clone()
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn open(&mut self, node: &Path) -> io::Result<FakeNode> {
        self.opened.lock().unwrap().push(node.to_path_buf());
        let Some(outcome) = self.opens.pop_front() else {
            self.messages.send(ReaderMessage::Shutdown).unwrap();
            return Err(io::ErrorKind::NotFound.into());
        };
        outcome.map_err(io::Error::from)
    }
}

struct Harness {
    clock: FakeMonotonic,
    sender: Sender<ReaderMessage>,
    receiver: Receiver<ReaderMessage>,
    timeouts: Arc<Mutex<Vec<Duration>>>,
    opened: Arc<Mutex<Vec<PathBuf>>>,
    discoveries: Arc<Mutex<usize>>,
}

impl Harness {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            clock: FakeMonotonic::default(),
            sender,
            receiver,
            timeouts: Arc::default(),
            opened: Arc::default(),
            discoveries: Arc::default(),
        }
    }

    /// A node with mode 1 held and `steps` to replay.
    fn node(&self, steps: Vec<Step>) -> FakeNode {
        FakeNode {
            held: vec![MODE_1],
            abs: Vec::new(),
            steps: steps.into(),
            clock: self.clock.clone(),
            messages: self.sender.clone(),
            timeouts: Arc::clone(&self.timeouts),
            sleeps: false,
            shuts_down_when_idle: true,
            queued: Vec::new(),
            failure: None,
        }
    }

    fn source(
        &self,
        lists: Vec<Vec<NodeEntry>>,
        opens: Vec<Result<FakeNode, io::ErrorKind>>,
    ) -> FakeSource {
        FakeSource {
            lists: lists.into(),
            last: Vec::new(),
            opens: opens.into(),
            opened: Arc::clone(&self.opened),
            discoveries: Arc::clone(&self.discoveries),
            messages: self.sender.clone(),
        }
    }

    /// Selects `device(1)`, runs the reader over `source` until it stops and returns its events.
    fn run(self, source: FakeSource) -> (Vec<ReaderEvent>, Self) {
        let (events, received) = async_channel::unbounded::<ReaderEvent>();
        self.sender
            .send(ReaderMessage::Selected(device(1)))
            .unwrap();
        Reader::new(source, self.clock.clone(), events).run(&self.receiver);
        let events = std::iter::from_fn(|| received.try_recv().ok()).collect();
        (events, self)
    }
}

/// The range the fake node reports for an axis: the code in the maximum, so a test can tell the
/// axes apart.
fn fake_range(code: u16) -> AxisRange {
    AxisRange {
        min: 0,
        max: 100 + i32::from(code),
    }
}

fn opened(number: u32) -> ReaderEvent {
    ReaderEvent::NodeOpened {
        generation: Generation(1),
        node: PathBuf::from(format!("/dev/input/event{number}")),
    }
}

fn mode_event(mode: Mode) -> ReaderEvent {
    ReaderEvent::Mode {
        generation: Generation(1),
        mode,
    }
}

/// `events` without the held-input events, for the tests of the mode and of the node.
fn without_held(events: &[ReaderEvent]) -> Vec<ReaderEvent> {
    events
        .iter()
        .filter(|event| !matches!(event, ReaderEvent::Held { .. }))
        .cloned()
        .collect()
}

fn held(change: HeldChange) -> ReaderEvent {
    ReaderEvent::Held {
        generation: Generation(1),
        change,
    }
}

fn snapshot(held: HeldInputs) -> ReaderEvent {
    self::held(HeldChange::Snapshot(held))
}

fn closed() -> ReaderEvent {
    ReaderEvent::NodeClosed {
        generation: Generation(1),
    }
}

/// Every event of an evtest capture. A line looks like
/// `Event: time 1793716445.329008, type 1 (EV_KEY), code 715 (BTN_TRIGGER_HAPPY12), value 0`;
/// the header and the `SYN_REPORT` separators are skipped.
fn evtest_events(capture: &str) -> Vec<InputEvent> {
    capture
        .lines()
        .filter_map(|line| line.strip_prefix("Event: time "))
        .filter_map(|line| {
            let mut fields = line.split(", ");
            let time = fields.next()?;
            let event_type = fields.next()?.strip_prefix("type ")?.split(' ').next()?;
            let code = fields.next()?.strip_prefix("code ")?.split(' ').next()?;
            let value = fields.next()?.strip_prefix("value ")?;
            let (seconds, micros) = time.split_once('.')?;
            Some(InputEvent {
                event_type: event_type.parse().unwrap(),
                code: code.parse().unwrap(),
                // MSC_SCAN values are hexadecimal; only key and axis values matter here.
                value: value.parse().unwrap_or(0),
                at: Duration::from_secs(seconds.parse().unwrap())
                    + Duration::from_micros(micros.parse().unwrap()),
            })
        })
        .collect()
}

#[test]
fn the_recorded_bounce_gives_one_change_per_selector_position() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("x52/tests/testdata/input/mode-bounce.evtest");
    let events = evtest_events(&fs::read_to_string(path).unwrap());
    assert!(events.len() > 50, "{} events", events.len());
    let harness = Harness::new();
    // The capture starts in mode 1, held when the node is opened just before its first event.
    harness
        .clock
        .set(events[0].at.checked_sub(Duration::from_millis(1)).unwrap());
    let node = harness.node(events.into_iter().map(Step::Event).collect());
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    // Pass 1 and 2: 2, 3, 1. Pass 3: 2 was held for only 160 ms, so 3 follows 1. The 2 that the
    // selector passes through from 3 to 1 is never applied, and the last 1 is applied by the poll
    // timeout, with no further event.
    let modes = [
        Mode::One,
        Mode::Two,
        Mode::Three,
        Mode::One,
        Mode::Two,
        Mode::Three,
        Mode::One,
        Mode::Three,
        Mode::One,
    ];
    let mut expected = vec![opened(20)];
    expected.extend(modes.map(mode_event));
    expected.push(closed());
    assert_eq!(without_held(&events), expected);
}

#[test]
fn buttons_and_pov1_change_the_held_inputs_and_change_no_mode() {
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let abs = |code, value, at| {
        Step::Event(InputEvent {
            event_type: EV_ABS,
            code,
            value,
            at,
        })
    };
    let node = harness.node(vec![
        Step::Event(key(288, true, ms(10))),
        // The stick: dropped.
        abs(0x00, 512, ms(20)),
        // A repeat of the held trigger: dropped.
        Step::Event(InputEvent {
            event_type: EV_KEY,
            code: 288,
            value: 2,
            at: ms(25),
        }),
        Step::Event(key(288, false, ms(30))),
        // POV 1 up, then up-right, then the same value again: dropped.
        abs(0x11, -1, ms(40)),
        abs(0x10, 1, ms(50)),
        abs(0x10, 1, ms(60)),
        Step::Event(key(301, true, ms(400))),
        Step::Event(key(301, false, ms(900))),
    ]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    let key_change = |button, pressed| held(HeldChange::Key { button, pressed });
    assert_eq!(
        events,
        [
            opened(20),
            mode_event(Mode::One),
            snapshot(HeldInputs::new()),
            key_change(Button::TriggerStage1, true),
            key_change(Button::TriggerStage1, false),
            held(HeldChange::Pov1 { x: 0, y: -1 }),
            held(HeldChange::Pov1 { x: 1, y: -1 }),
            key_change(Button::T6, true),
            key_change(Button::T6, false),
            closed()
        ]
    );
}

#[test]
fn the_snapshot_holds_the_keys_and_pov1_at_the_open() {
    let harness = Harness::new();
    let mut node = harness.node(vec![Step::Event(key(
        MODE_1 + 1,
        true,
        Duration::from_millis(10),
    ))]);
    // Fire and A held with mode 1, POV 1 left.
    node.held = vec![290, MODE_1, 289];
    node.abs = vec![(0x10, -1)];
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    let map = InputMap::for_device(DeviceType::X52Pro);
    let expected = HeldInputs::from_snapshot(&map, &[289, 290, MODE_1], (-1, 0));
    assert_eq!(
        expected.last(),
        Some(x52::input::Input::Pov1(x52::input::Pov1Direction::Left))
    );
    assert_eq!(
        events,
        [
            opened(20),
            mode_event(Mode::One),
            snapshot(expected),
            // A mode key is never a held-input change.
            closed()
        ]
    );
}

#[test]
fn the_node_appearing_a_poll_after_the_usb_device_is_found() {
    let harness = Harness::new();
    let node = harness.node(Vec::new());
    let source = harness.source(vec![Vec::new(), vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, harness) = harness.run(source);

    assert_eq!(
        without_held(&events),
        [opened(20), mode_event(Mode::One), closed()]
    );
    assert!(*harness.discoveries.lock().unwrap() >= 2);
}

#[test]
fn a_node_denied_at_first_is_opened_once_readable() {
    let harness = Harness::new();
    let node = harness.node(Vec::new());
    let source = harness.source(
        vec![vec![entry(20, USB)]],
        vec![Err(io::ErrorKind::PermissionDenied), Ok(node)],
    );

    let (events, harness) = harness.run(source);

    assert_eq!(
        without_held(&events),
        [opened(20), mode_event(Mode::One), closed()]
    );
    assert_eq!(
        harness.opened.lock().unwrap().len(),
        2,
        "denied, then opened"
    );
}

#[test]
fn enodev_while_the_usb_device_stays_selected_reopens_the_node_when_it_is_back() {
    let harness = Harness::new();
    let first = harness.node(vec![Step::Fail(ENODEV)]);
    let second = harness.node(Vec::new());
    let source = harness.source(
        vec![vec![entry(20, USB)], Vec::new(), vec![entry(21, USB)]],
        vec![Ok(first), Ok(second)],
    );

    let (events, _) = harness.run(source);

    assert_eq!(
        without_held(&events),
        [
            opened(20),
            mode_event(Mode::One),
            closed(),
            opened(21),
            mode_event(Mode::One),
            closed(),
        ]
    );
}

#[test]
fn of_two_devices_the_node_of_the_selected_one_is_used() {
    let harness = Harness::new();
    let node = harness.node(Vec::new());
    // usb1/1-40 shares the text prefix of usb1/1-4 but is another device.
    let other = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-40";
    let source = harness.source(vec![vec![entry(18, other), entry(20, USB)]], vec![Ok(node)]);

    let (events, harness) = harness.run(source);

    assert_eq!(events[0], opened(20));
    assert_eq!(
        harness.opened.lock().unwrap()[0],
        PathBuf::from("/dev/input/event20")
    );
}

#[test]
fn gone_for_the_selection_stops_and_a_stale_gone_does_not() {
    let harness = Harness::new();
    let node = harness.node(vec![
        Step::Messages(vec![ReaderMessage::Gone(Generation(7))]),
        Step::Messages(vec![
            ReaderMessage::Gone(Generation(1)),
            ReaderMessage::Shutdown,
        ]),
    ]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, harness) = harness.run(source);

    assert_eq!(
        without_held(&events),
        [opened(20), mode_event(Mode::One), closed()]
    );
    assert_eq!(
        harness.timeouts.lock().unwrap().len(),
        2,
        "the stale Gone kept it reading"
    );
}

#[test]
fn a_new_selection_closes_the_node_and_follows_the_new_one() {
    let harness = Harness::new();
    let mut replugged = device(2);
    replugged.attachment.address = 6;
    let first = harness.node(vec![Step::Messages(vec![ReaderMessage::Selected(
        replugged,
    )])]);
    let second = harness.node(Vec::new());
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(first), Ok(second)]);

    let (events, _) = harness.run(source);

    assert_eq!(
        without_held(&events),
        [
            opened(20),
            mode_event(Mode::One),
            closed(),
            ReaderEvent::NodeOpened {
                generation: Generation(2),
                node: PathBuf::from("/dev/input/event20"),
            },
            ReaderEvent::Mode {
                generation: Generation(2),
                mode: Mode::One,
            },
            ReaderEvent::NodeClosed {
                generation: Generation(2),
            },
        ]
    );
}

#[test]
fn an_idle_poll_waits_at_most_250_ms_and_then_sees_shutdown() {
    let harness = Harness::new();
    let node = harness.node(vec![Step::Messages(vec![ReaderMessage::Shutdown])]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (_, harness) = harness.run(source);

    assert_eq!(*harness.timeouts.lock().unwrap(), [POLL_CAP]);
    assert_eq!(POLL_CAP, Duration::from_millis(250));
}

#[test]
fn shutdown_from_an_idle_poll_returns_within_250_ms() {
    let harness = Harness::new();
    let mut node = harness.node(Vec::new());
    node.sleeps = true;
    node.shuts_down_when_idle = false;
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);
    let sender = harness.sender.clone();
    let (done, finished) = mpsc::channel();
    let reader = thread::spawn(move || {
        let _ = harness.run(source);
        done.send(()).unwrap();
    });
    // Let it reach its idle poll first.
    thread::sleep(Duration::from_millis(50));

    sender.send(ReaderMessage::Shutdown).unwrap();

    finished
        .recv_timeout(POLL_CAP)
        .expect("the reader did not stop within 250 ms");
    reader.join().unwrap();
}

#[test]
fn the_poll_timeout_is_the_pending_deadline_on_monotonic_time() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    let mut tracker = ModeTracker::new(map, &[MODE_1]);
    assert_eq!(tracker.active(), Some(Mode::One));
    assert_eq!(tracker.timeout(Duration::ZERO), POLL_CAP);

    // Mode 2 pressed at a wall-clock timestamp, read at monotonic 10 s; mode 1 released.
    let wall = Duration::from_secs(1_793_716_445);
    let read_at = Duration::from_secs(10);
    assert_eq!(tracker.on_event(&key(MODE_1, false, wall), read_at), None);
    assert_eq!(tracker.on_event(&key(716, true, wall), read_at), None);

    let ms = Duration::from_millis;
    assert_eq!(tracker.timeout(read_at + ms(50)), ms(150));
    assert_eq!(tracker.on_time(read_at + ms(199)), None);
    assert_eq!(tracker.on_time(read_at + ms(200)), Some(Mode::Two));
    assert_eq!(tracker.timeout(read_at + ms(200)), POLL_CAP);
}

#[test]
fn a_node_belongs_to_the_device_whose_directory_contains_it() {
    let nodes = [
        entry(18, "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-40"),
        entry(20, USB),
    ];
    assert_eq!(node_of(&nodes, Path::new(USB)), Some(&nodes[1]));
    assert_eq!(
        node_of(
            &nodes,
            Path::new("/sys/devices/pci0000:00/0000:00:14.0/usb1/1-6")
        ),
        None
    );
}

fn abs(code: u16, value: i32, at: Duration) -> InputEvent {
    InputEvent {
        event_type: EV_ABS,
        code,
        value,
        at,
    }
}

/// The axes of the X52 Pro other than POV 1, as the fake node reports them, at `values` (by code;
/// every other axis is `0`).
fn axis_snapshot_of(generation: u64, values: &[(u16, i32)]) -> ReaderEvent {
    let map = InputMap::for_device(DeviceType::X52Pro);
    let axes = map
        .axes()
        .filter(|(_, axis)| !matches!(axis, Axis::Pov1X | Axis::Pov1Y))
        .map(|(code, axis)| {
            let value = values
                .iter()
                .find(|(known, _)| *known == code)
                .map_or(0, |(_, value)| *value);
            (
                axis,
                AxisState {
                    range: fake_range(code),
                    value,
                },
            )
        })
        .collect();
    ReaderEvent::Axes {
        generation: Generation(generation),
        change: AxisChange::Snapshot(axes),
    }
}

fn axis_values(values: Vec<(Axis, i32)>) -> ReaderEvent {
    ReaderEvent::Axes {
        generation: Generation(1),
        change: AxisChange::Values(values),
    }
}

/// Only the axis events of `events`.
fn only_axes(events: &[ReaderEvent]) -> Vec<ReaderEvent> {
    events
        .iter()
        .filter(|event| matches!(event, ReaderEvent::Axes { .. }))
        .cloned()
        .collect()
}

#[test]
fn no_axis_is_reported_while_nothing_listens() {
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let node = harness.node(vec![
        Step::Event(abs(0x00, 512, ms(10))),
        Step::Batch(vec![abs(0x02, 40, ms(20)), abs(0x28, 3, ms(20))]),
    ]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    assert_eq!(only_axes(&events), []);
}

#[test]
fn listening_reports_a_snapshot_then_one_change_per_read() {
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let mut node = harness.node(vec![
        // The throttle twice and the stick in one read: one event, the throttle's last value.
        Step::Batch(vec![
            abs(0x02, 40, ms(10)),
            abs(0x00, 600, ms(10)),
            abs(0x02, 42, ms(10)),
            key(289, true, ms(10)),
        ]),
        // POV 1 stays a held-input change.
        Step::Event(abs(0x10, 1, ms(20))),
    ]);
    node.abs = vec![(0x00, 511), (0x06, 31)];
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);
    harness.sender.send(ReaderMessage::ListenToAxes).unwrap();

    let (events, _) = harness.run(source);

    assert_eq!(
        only_axes(&events),
        [
            axis_snapshot_of(1, &[(0x00, 511), (0x06, 31)]),
            axis_values(vec![(Axis::StickX, 600), (Axis::Throttle, 42)]),
        ]
    );
    let snapshot_at = events
        .iter()
        .position(|event| matches!(event, ReaderEvent::Axes { .. }))
        .unwrap();
    assert!(
        matches!(
            events[snapshot_at - 1],
            ReaderEvent::Held {
                change: HeldChange::Snapshot(_),
                ..
            }
        ),
        "the axis snapshot follows the held snapshot: {events:?}"
    );
}

#[test]
fn listening_starts_and_stops_on_an_open_node() {
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let node = harness.node(vec![
        Step::Event(abs(0x00, 100, ms(10))),
        Step::Messages(vec![ReaderMessage::ListenToAxes]),
        Step::Event(abs(0x00, 200, ms(30))),
        Step::Messages(vec![ReaderMessage::IgnoreAxes]),
        Step::Event(abs(0x00, 300, ms(50))),
    ]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    assert_eq!(
        only_axes(&events),
        [
            axis_snapshot_of(1, &[]),
            axis_values(vec![(Axis::StickX, 200)]),
        ]
    );
}

#[test]
fn a_reopened_node_gets_a_new_axis_snapshot_while_listening() {
    let harness = Harness::new();
    let first = harness.node(vec![Step::Fail(ENODEV)]);
    let mut second = harness.node(Vec::new());
    second.abs = vec![(0x02, 77)];
    let source = harness.source(
        vec![vec![entry(20, USB)], Vec::new(), vec![entry(21, USB)]],
        vec![Ok(first), Ok(second)],
    );
    harness.sender.send(ReaderMessage::ListenToAxes).unwrap();

    let (events, _) = harness.run(source);

    assert_eq!(
        only_axes(&events),
        [axis_snapshot_of(1, &[]), axis_snapshot_of(1, &[(0x02, 77)])]
    );
}

#[test]
fn listening_survives_a_new_selection() {
    let harness = Harness::new();
    let first = harness.node(vec![Step::Messages(vec![ReaderMessage::Selected(device(
        2,
    ))])]);
    let second = harness.node(Vec::new());
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(first), Ok(second)]);
    harness.sender.send(ReaderMessage::ListenToAxes).unwrap();

    let (events, _) = harness.run(source);

    assert_eq!(
        only_axes(&events),
        [axis_snapshot_of(1, &[]), axis_snapshot_of(2, &[])]
    );
}

#[test]
fn a_listen_right_after_an_ignore_sends_a_new_snapshot() {
    // Leaving the tester and coming back within one poll: both messages are drained together, and
    // the main thread has forgotten the axes in between.
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let mut node = harness.node(vec![
        Step::Messages(vec![ReaderMessage::IgnoreAxes, ReaderMessage::ListenToAxes]),
        Step::Event(abs(0x00, 300, ms(50))),
    ]);
    node.abs = vec![(0x00, 200)];
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);
    harness.sender.send(ReaderMessage::ListenToAxes).unwrap();

    let (events, _) = harness.run(source);

    assert_eq!(
        only_axes(&events),
        [
            axis_snapshot_of(1, &[(0x00, 200)]),
            axis_snapshot_of(1, &[(0x00, 200)]),
            axis_values(vec![(Axis::StickX, 300)]),
        ]
    );
}
