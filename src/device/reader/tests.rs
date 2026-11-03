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
    /// The next read fails with this `errno`.
    Fail(i32),
    /// These messages are sent to the reader during the wait.
    Messages(Vec<ReaderMessage>),
}

struct FakeNode {
    held: Vec<u16>,
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
            Some(Step::Event(_)) => {
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
        let (events, received) = async_channel::unbounded();
        self.sender
            .send(ReaderMessage::Selected(device(1)))
            .unwrap();
        Reader::new(source, self.clock.clone(), events).run(&self.receiver);
        let events = std::iter::from_fn(|| received.try_recv().ok()).collect();
        (events, self)
    }
}

fn opened(number: u32) -> ReaderEvent {
    ReaderEvent::NodeOpened {
        generation: Generation(1),
        node: PathBuf::from(format!("/dev/input/event{number}")),
    }
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
    expected.extend(modes.map(ReaderEvent::Mode));
    expected.push(closed());
    assert_eq!(events, expected);
}

#[test]
fn other_buttons_and_axes_change_nothing() {
    let harness = Harness::new();
    let ms = Duration::from_millis;
    let node = harness.node(vec![
        Step::Event(key(288, true, ms(10))),
        Step::Event(InputEvent {
            event_type: EV_ABS,
            code: 0x00,
            value: 512,
            at: ms(20),
        }),
        Step::Event(key(288, false, ms(30))),
        Step::Event(key(301, true, ms(400))),
        Step::Event(key(301, false, ms(900))),
    ]);
    let source = harness.source(vec![vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, _) = harness.run(source);

    assert_eq!(events, [opened(20), ReaderEvent::Mode(Mode::One), closed()]);
}

#[test]
fn the_node_appearing_a_poll_after_the_usb_device_is_found() {
    let harness = Harness::new();
    let node = harness.node(Vec::new());
    let source = harness.source(vec![Vec::new(), vec![entry(20, USB)]], vec![Ok(node)]);

    let (events, harness) = harness.run(source);

    assert_eq!(events, [opened(20), ReaderEvent::Mode(Mode::One), closed()]);
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

    assert_eq!(events, [opened(20), ReaderEvent::Mode(Mode::One), closed()]);
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
        events,
        [
            opened(20),
            ReaderEvent::Mode(Mode::One),
            closed(),
            opened(21),
            ReaderEvent::Mode(Mode::One),
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

    assert_eq!(events, [opened(20), ReaderEvent::Mode(Mode::One), closed()]);
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
        events,
        [
            opened(20),
            ReaderEvent::Mode(Mode::One),
            closed(),
            ReaderEvent::NodeOpened {
                generation: Generation(2),
                node: PathBuf::from("/dev/input/event20"),
            },
            ReaderEvent::Mode(Mode::One),
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
