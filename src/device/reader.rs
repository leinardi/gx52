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

//! The evdev reader: the mode selector and the held buttons of the selected device
//! (docs/architecture.md, "Threads and data flow", "evdev reader thread").
//!
//! The reader follows the device watch's selection. USB presence and evdev readiness are tracked
//! apart: the evdev node can appear a poll after the USB device (S4), be unreadable at first, or
//! go away while the USB device stays listed, and in each case the reader looks for it again every
//! [`DISCOVERY_INTERVAL`]. A node is used only when its sysfs device lies under the selected USB
//! device's sysfs directory, so USB and evdev always come from the same physical device. The node
//! is opened without grabbing (games keep every input), and nothing is logged per event.
//!
//! The axes other than POV 1 are reported only while the input tester listens
//! ([`ReaderMessage::ListenToAxes`]), coalesced to one [`ReaderEvent::Axes`] per read.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, UNIX_EPOCH};

use rustix::event::{PollFd, PollFlags, Timespec};
use x52::input::{
    Axis, AxisRange, AxisState, AxisValues, Button, HeldInputs, InputMap, Mode, ModeDebouncer,
    ModeSet,
};

use super::id::{DeviceId, Generation};
use super::monotonic::Monotonic;

/// How often the reader looks for the selected device's evdev node while it has none.
pub(crate) const DISCOVERY_INTERVAL: Duration = Duration::from_millis(250);

/// The longest poll on an open node, so a shutdown or a new selection is noticed within it.
pub(crate) const POLL_CAP: Duration = Duration::from_millis(250);

/// The evdev event type of a key or button (`EV_KEY`).
const EV_KEY: u16 = 0x01;

/// The evdev event type of an absolute axis (`EV_ABS`).
const EV_ABS: u16 = 0x03;

/// The value of an `EV_KEY` event that repeats a held key.
const KEY_REPEAT: i32 = 2;

/// The `errno` of a device that went away.
const ENODEV: i32 = 19;

/// A message to the reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReaderMessage {
    /// The device watch selected a device, or a new attachment of it.
    Selected(DeviceId),
    /// The selection of that generation is gone.
    Gone(Generation),
    /// The input tester shows the axes: report them, starting with a snapshot.
    ListenToAxes,
    /// Nothing shows the axes any more: stop reporting them.
    IgnoreAxes,
    /// Stop the thread.
    Shutdown,
}

/// What the reader reports to the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReaderEvent {
    /// The selected device's evdev node is open.
    NodeOpened {
        /// The selection it belongs to.
        generation: Generation,
        /// The node, `/dev/input/eventN`.
        node: PathBuf,
    },
    /// The node was closed: gone, unreadable or no longer selected.
    NodeClosed {
        /// The selection it belonged to.
        generation: Generation,
    },
    /// The active mode: the initial one read with `EVIOCGKEY`, then every debounced change.
    Mode {
        /// The selection it belongs to.
        generation: Generation,
        /// The mode.
        mode: Mode,
    },
    /// The held buttons and POV 1 directions changed.
    Held {
        /// The selection it belongs to.
        generation: Generation,
        /// What changed.
        change: HeldChange,
    },
    /// The axes other than POV 1 changed; sent only while the input tester listens.
    Axes {
        /// The selection it belongs to.
        generation: Generation,
        /// What changed.
        change: AxisChange,
    },
}

/// A change of the axes, for [`AxisValues`] on the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AxisChange {
    /// Every axis of the node, with its range and value (`EVIOCGABS`): sent when listening starts
    /// on an open node and on every open while listening.
    Snapshot(Vec<(Axis, AxisState)>),
    /// The last value of each axis that moved within one read.
    Values(Vec<(Axis, i32)>),
}

impl AxisChange {
    /// Applies the change to `axes`.
    pub(crate) fn apply(&self, axes: &mut AxisValues) {
        match self {
            Self::Snapshot(snapshot) => axes.set_snapshot(snapshot.iter().copied()),
            Self::Values(values) => {
                for (axis, value) in values {
                    axes.set_value(*axis, *value);
                }
            }
        }
    }
}

/// A change of the held inputs, for [`HeldInputs`] on the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HeldChange {
    /// Everything held when the node opened (`EVIOCGKEY` and `EVIOCGABS`).
    Snapshot(HeldInputs),
    /// A button other than the mode selector was pressed or released (repeats are dropped).
    Key {
        /// The button.
        button: Button,
        /// Pressed, or released.
        pressed: bool,
    },
    /// POV 1 moved to `x`, `y` (each −1, 0 or 1).
    Pov1 {
        /// `ABS_HAT0X`: −1 left, 1 right.
        x: i32,
        /// `ABS_HAT0Y`: −1 up, 1 down.
        y: i32,
    },
}

impl HeldChange {
    /// Applies the change to `held`.
    pub(crate) fn apply(&self, held: &mut HeldInputs) {
        match self {
            Self::Snapshot(snapshot) => snapshot.clone_into(held),
            Self::Key { button, pressed } => {
                let input = x52::input::Input::Button(*button);
                if *pressed {
                    held.press(input);
                } else {
                    held.release(input);
                }
            }
            Self::Pov1 { x, y } => held.set_pov1(*x, *y),
        }
    }
}

/// One input event, reduced to what the reader needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputEvent {
    /// The evdev event type.
    pub(crate) event_type: u16,
    /// The code within the type.
    pub(crate) code: u16,
    /// The value: for a key, `0` released, `1` pressed, `2` repeated.
    pub(crate) value: i32,
    /// The event's timestamp, as evdev sets it.
    pub(crate) at: Duration,
}

/// An open evdev node.
pub(crate) trait InputNode {
    /// The codes of the keys held now (`EVIOCGKEY`).
    ///
    /// # Errors
    ///
    /// The I/O error of the ioctl.
    fn held_keys(&self) -> io::Result<Vec<u16>>;

    /// The range and value of the absolute axis `code` now (`EVIOCGABS`); all `0` for an axis the
    /// node does not have.
    ///
    /// # Errors
    ///
    /// The I/O error of the ioctl.
    fn abs_info(&self, code: u16) -> io::Result<AxisState>;

    /// Waits up to `timeout` for events; true when there may be some to read.
    ///
    /// # Errors
    ///
    /// The I/O error of the wait.
    fn wait(&mut self, timeout: Duration) -> io::Result<bool>;

    /// The events queued now, without blocking.
    ///
    /// # Errors
    ///
    /// The I/O error of the read, `ENODEV` once the device is gone.
    fn read(&mut self) -> io::Result<Vec<InputEvent>>;
}

/// An evdev node and the sysfs device it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeEntry {
    /// The node, `/dev/input/eventN`.
    pub(crate) node: PathBuf,
    /// `/sys/class/input/eventN/device`, canonicalised.
    pub(crate) device: PathBuf,
}

/// Lists and opens evdev nodes; the reader is generic over it so the tests can use fakes.
pub(crate) trait EvdevSource {
    /// An open node.
    type Node: InputNode;

    /// Every evdev node now present.
    fn nodes(&mut self) -> Vec<NodeEntry>;

    /// `path` with every symbolic link resolved, as [`NodeEntry::device`] is.
    fn canonical(&self, path: &Path) -> PathBuf;

    /// Opens `node` for reading, without grabbing it.
    ///
    /// # Errors
    ///
    /// The I/O error of the open, `EACCES` without permission.
    fn open(&mut self, node: &Path) -> io::Result<Self::Node>;
}

/// The node of `device` among `nodes`: the first whose sysfs device lies under the selected USB
/// device's directory `usb`.
pub(crate) fn node_of<'node>(nodes: &'node [NodeEntry], usb: &Path) -> Option<&'node NodeEntry> {
    nodes.iter().find(|entry| entry.device.starts_with(usb))
}

/// The mode debounce over evdev timestamps, with the poll timeout it asks for.
///
/// evdev timestamps follow the wall clock, which can jump; the time between events is measured
/// with monotonic time from the last key event, so a pending mode is applied on time.
#[derive(Debug)]
pub(crate) struct ModeTracker {
    map: InputMap,
    debouncer: ModeDebouncer,
    /// The timestamp of the last key event and the monotonic time it was read at.
    anchor: Option<(Duration, Duration)>,
}

impl ModeTracker {
    /// A tracker starting from the keys held in the `EVIOCGKEY` snapshot.
    pub(crate) fn new(map: InputMap, held: &[u16]) -> Self {
        let modes: ModeSet = held.iter().filter_map(|code| map.mode_for(*code)).collect();
        Self {
            map,
            debouncer: ModeDebouncer::new(modes, Duration::ZERO),
            anchor: None,
        }
    }

    /// The active mode, if one is.
    pub(crate) const fn active(&self) -> Option<Mode> {
        self.debouncer.active()
    }

    /// Applies `event`, read at monotonic time `read_at`; returns the mode it applied, if any.
    pub(crate) fn on_event(&mut self, event: &InputEvent, read_at: Duration) -> Option<Mode> {
        if event.event_type != EV_KEY {
            return None;
        }
        let mode = self.map.mode_for(event.code)?;
        self.anchor = Some((event.at, read_at));
        self.debouncer.on_event(mode, event.value != 0, event.at)
    }

    /// Applies the pending mode if its time has come at monotonic time `now`.
    pub(crate) fn on_time(&mut self, now: Duration) -> Option<Mode> {
        let event_now = self.event_time(now)?;
        self.debouncer.advance_to(event_now)
    }

    /// How long to poll at monotonic time `now`: until the pending mode is due, at most
    /// [`POLL_CAP`].
    pub(crate) fn timeout(&self, now: Duration) -> Duration {
        match (self.debouncer.deadline(), self.event_time(now)) {
            (Some(deadline), Some(event_now)) => deadline.saturating_sub(event_now).min(POLL_CAP),
            (Some(_) | None, _) => POLL_CAP,
        }
    }

    /// Monotonic time `now` on the evdev timestamps' scale.
    fn event_time(&self, now: Duration) -> Option<Duration> {
        let (event_at, read_at) = self.anchor?;
        Some(event_at.saturating_add(now.saturating_sub(read_at)))
    }
}

/// Turns the key and POV 1 events into changes of the held inputs. Other axes are dropped.
#[derive(Debug)]
pub(crate) struct HeldTracker {
    map: InputMap,
    /// POV 1's `x`, `y` as last reported.
    pov1: (i32, i32),
}

impl HeldTracker {
    /// A tracker starting from POV 1 at `pov1`.
    pub(crate) const fn new(map: InputMap, pov1: (i32, i32)) -> Self {
        Self { map, pov1 }
    }

    /// The change `event` makes, if any.
    pub(crate) fn on_event(&mut self, event: &InputEvent) -> Option<HeldChange> {
        match event.event_type {
            EV_KEY => {
                let button = self.map.button(event.code)?;
                if self.map.mode_for(event.code).is_some() || event.value == KEY_REPEAT {
                    return None;
                }
                Some(HeldChange::Key {
                    button,
                    pressed: event.value != 0,
                })
            }
            EV_ABS => {
                let (x, y) = match self.map.axis(event.code)? {
                    Axis::Pov1X => (event.value, self.pov1.1),
                    Axis::Pov1Y => (self.pov1.0, event.value),
                    Axis::StickX
                    | Axis::StickY
                    | Axis::Twist
                    | Axis::Throttle
                    | Axis::RotaryI
                    | Axis::RotaryE
                    | Axis::Slider
                    | Axis::MinistickX
                    | Axis::MinistickY => return None,
                };
                if (x, y) == self.pov1 {
                    return None;
                }
                self.pov1 = (x, y);
                Some(HeldChange::Pov1 { x, y })
            }
            _ => None,
        }
    }
}

/// Collects the values of the axes other than POV 1 within one read, the last value per axis.
#[derive(Debug)]
pub(crate) struct AxisTracker {
    map: InputMap,
    moved: BTreeMap<Axis, i32>,
}

impl AxisTracker {
    /// A tracker of the axes of `map`.
    pub(crate) const fn new(map: InputMap) -> Self {
        Self {
            map,
            moved: BTreeMap::new(),
        }
    }

    /// Notes the value of an axis event other than POV 1's.
    pub(crate) fn on_event(&mut self, event: &InputEvent) {
        if event.event_type != EV_ABS {
            return;
        }
        if let Some(axis) = self.map.axis(event.code).filter(|axis| !is_pov1(*axis)) {
            self.moved.insert(axis, event.value);
        }
    }

    /// The values noted since the last call, if any.
    pub(crate) fn take(&mut self) -> Option<AxisChange> {
        if self.moved.is_empty() {
            return None;
        }
        Some(AxisChange::Values(
            std::mem::take(&mut self.moved).into_iter().collect(),
        ))
    }
}

/// Whether `axis` is one of POV 1's, which the held inputs report.
const fn is_pov1(axis: Axis) -> bool {
    matches!(axis, Axis::Pov1X | Axis::Pov1Y)
}

/// Every axis of `map` other than POV 1's, with its range and value on `node` now.
fn axis_snapshot(node: &impl InputNode, map: &InputMap) -> io::Result<AxisChange> {
    let axes = map
        .axes()
        .filter(|(_, axis)| !is_pov1(*axis))
        .map(|(code, axis)| Ok((axis, node.abs_info(code)?)))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(AxisChange::Snapshot(axes))
}

/// The evdev codes of POV 1's two axes in `map`.
fn pov1_codes(map: &InputMap) -> (Option<u16>, Option<u16>) {
    let code = |wanted| {
        map.axes()
            .find(|(_, axis)| *axis == wanted)
            .map(|(code, _)| code)
    };
    (code(Axis::Pov1X), code(Axis::Pov1Y))
}

/// POV 1's `x`, `y` on `node` now.
fn pov1_state(node: &impl InputNode, map: &InputMap) -> io::Result<(i32, i32)> {
    let value = |code: Option<u16>| -> io::Result<i32> {
        code.map_or(Ok(0), |code| Ok(node.abs_info(code)?.value))
    };
    let (x, y) = pov1_codes(map);
    Ok((value(x)?, value(y)?))
}

/// What ends following one selection.
#[derive(Debug, PartialEq, Eq)]
enum Next {
    /// The selection is gone; wait for the next one.
    Gone,
    /// Another device, or another attachment, is selected.
    Selected(DeviceId),
    /// Stop the thread.
    Shutdown,
}

/// Why reading a node stopped.
enum Stop {
    /// The selection changed or the thread stops.
    Next(Next),
    /// The node went away or could not be read; look for it again.
    NodeLost,
}

/// The reader, driven by [`Reader::run`]. It reports into a channel of any event type a
/// [`ReaderEvent`] converts to.
pub(crate) struct Reader<S, M, E> {
    source: S,
    monotonic: M,
    events: async_channel::Sender<E>,
    /// Whether the input tester listens to the axes; kept across nodes and selections.
    axes: Listening,
}

/// Whether the axes are reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listening {
    /// Report them.
    On,
    /// Drop them.
    Off,
}

impl<S: EvdevSource, M: Monotonic, E: From<ReaderEvent>> Reader<S, M, E> {
    /// A reader over `source` reporting to `events`.
    pub(crate) const fn new(source: S, monotonic: M, events: async_channel::Sender<E>) -> Self {
        Self {
            source,
            monotonic,
            events,
            axes: Listening::Off,
        }
    }

    /// Runs until [`ReaderMessage::Shutdown`] or until every sender is gone.
    pub(crate) fn run(mut self, messages: &Receiver<ReaderMessage>) {
        let mut selected: Option<DeviceId> = None;
        loop {
            let next = match &selected {
                None => match messages.recv() {
                    Ok(ReaderMessage::Selected(device)) => Next::Selected(device),
                    Ok(ReaderMessage::Gone(_)) => Next::Gone,
                    Ok(ReaderMessage::ListenToAxes) => {
                        self.axes = Listening::On;
                        continue;
                    }
                    Ok(ReaderMessage::IgnoreAxes) => {
                        self.axes = Listening::Off;
                        continue;
                    }
                    Ok(ReaderMessage::Shutdown) | Err(_) => Next::Shutdown,
                },
                Some(device) => self.follow(device, messages),
            };
            match next {
                Next::Gone => selected = None,
                Next::Selected(device) => selected = Some(device),
                Next::Shutdown => return,
            }
        }
    }

    /// Looks for the node of `device` every [`DISCOVERY_INTERVAL`] and reads it while it is
    /// there, until the selection changes.
    fn follow(&mut self, device: &DeviceId, messages: &Receiver<ReaderMessage>) -> Next {
        let usb = self.source.canonical(&device.sysfs_path);
        loop {
            loop {
                match messages.try_recv() {
                    Ok(message) => {
                        if let Some(next) = self.on_message(message, device) {
                            return next;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Next::Shutdown,
                }
            }
            if let Some((node, path)) = self.discover(&usb) {
                match self.read(device, node, path, messages) {
                    Stop::Next(next) => return next,
                    Stop::NodeLost => {}
                }
            }
            match messages.recv_timeout(DISCOVERY_INTERVAL) {
                Ok(message) => {
                    if let Some(next) = self.on_message(message, device) {
                        return next;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Next::Shutdown,
            }
        }
    }

    /// Opens the node under `usb`, if there is one and it can be opened.
    fn discover(&mut self, usb: &Path) -> Option<(S::Node, PathBuf)> {
        let nodes = self.source.nodes();
        let entry = node_of(&nodes, usb)?;
        // Not logged: a node that is not readable yet is retried every 250 ms.
        let node = self.source.open(&entry.node).ok()?;
        Some((node, entry.node.clone()))
    }

    /// Reads `node` until it goes away or the selection changes.
    fn read(
        &mut self,
        device: &DeviceId,
        mut node: S::Node,
        path: PathBuf,
        messages: &Receiver<ReaderMessage>,
    ) -> Stop {
        let map = InputMap::for_device(device.device_type);
        let (Ok(held), Ok(pov1)) = (node.held_keys(), pov1_state(&node, &map)) else {
            return Stop::NodeLost;
        };
        let generation = device.generation;
        let mut tracker = ModeTracker::new(map, &held);
        let mut held_tracker = HeldTracker::new(map, pov1);
        let mut axis_tracker = AxisTracker::new(map);
        tracing::info!("Reading input from {}", path.display());
        self.send(ReaderEvent::NodeOpened {
            generation,
            node: path,
        });
        if let Some(mode) = tracker.active() {
            self.send(ReaderEvent::Mode { generation, mode });
        }
        self.send(ReaderEvent::Held {
            generation,
            change: HeldChange::Snapshot(HeldInputs::from_snapshot(&map, &held, pov1)),
        });
        let mut reported = Listening::Off;
        let stop = loop {
            if self.axes != reported {
                reported = self.axes;
                if reported == Listening::On {
                    let Ok(change) = axis_snapshot(&node, &map) else {
                        break Stop::NodeLost;
                    };
                    self.send(ReaderEvent::Axes { generation, change });
                }
            }
            let Ok(readable) = node.wait(tracker.timeout(self.monotonic.elapsed())) else {
                break Stop::NodeLost;
            };
            if let Some(next) = self.drain_messages(messages, device) {
                break Stop::Next(next);
            }
            if readable {
                match node.read() {
                    Ok(events) => {
                        let read_at = self.monotonic.elapsed();
                        for event in &events {
                            if let Some(mode) = tracker.on_event(event, read_at) {
                                self.send(ReaderEvent::Mode { generation, mode });
                            }
                            if let Some(change) = held_tracker.on_event(event) {
                                self.send(ReaderEvent::Held { generation, change });
                            }
                            if self.axes == Listening::On {
                                axis_tracker.on_event(event);
                            }
                        }
                        if let Some(change) = axis_tracker.take() {
                            self.send(ReaderEvent::Axes { generation, change });
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) => {
                        if error.raw_os_error() != Some(ENODEV) {
                            tracing::warn!("Cannot read the input events: {error}");
                        }
                        break Stop::NodeLost;
                    }
                }
            }
            if let Some(mode) = tracker.on_time(self.monotonic.elapsed()) {
                self.send(ReaderEvent::Mode { generation, mode });
            }
        };
        tracing::info!("Stopped reading input");
        self.send(ReaderEvent::NodeClosed { generation });
        stop
    }

    fn send(&self, event: ReaderEvent) {
        // An unbounded channel only fails once the main thread has stopped listening.
        let _ = self.events.try_send(event.into());
    }

    /// Handles every queued message; returns what ends following `device`, if one does.
    fn drain_messages(
        &mut self,
        messages: &Receiver<ReaderMessage>,
        device: &DeviceId,
    ) -> Option<Next> {
        loop {
            match messages.try_recv() {
                Ok(message) => {
                    if let Some(next) = self.on_message(message, device) {
                        return Some(next);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return Some(Next::Shutdown),
            }
        }
    }

    /// What `message` means while `device` is followed: `None` when it changes nothing.
    fn on_message(&mut self, message: ReaderMessage, device: &DeviceId) -> Option<Next> {
        match message {
            ReaderMessage::Selected(selected) if selected == *device => None,
            ReaderMessage::Selected(selected) => Some(Next::Selected(selected)),
            ReaderMessage::Gone(generation) if generation == device.generation => Some(Next::Gone),
            ReaderMessage::Gone(_) => None,
            ReaderMessage::ListenToAxes => {
                self.axes = Listening::On;
                None
            }
            ReaderMessage::IgnoreAxes => {
                self.axes = Listening::Off;
                None
            }
            ReaderMessage::Shutdown => Some(Next::Shutdown),
        }
    }
}

/// Where the kernel lists the input devices.
const SYS_CLASS_INPUT: &str = "/sys/class/input";

/// Where the evdev nodes are.
const DEV_INPUT: &str = "/dev/input";

/// The [`EvdevSource`] of the real system: sysfs and `/dev/input`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SysfsEvdev;

impl EvdevSource for SysfsEvdev {
    type Node = EvdevNode;

    fn nodes(&mut self) -> Vec<NodeEntry> {
        let Ok(entries) = fs::read_dir(SYS_CLASS_INPUT) else {
            return Vec::new();
        };
        let mut nodes: Vec<NodeEntry> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                if !name.to_str()?.starts_with("event") {
                    return None;
                }
                Some(NodeEntry {
                    node: Path::new(DEV_INPUT).join(&name),
                    device: fs::canonicalize(entry.path().join("device")).ok()?,
                })
            })
            .collect();
        nodes.sort_by(|left, right| left.node.cmp(&right.node));
        nodes
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn open(&mut self, node: &Path) -> io::Result<EvdevNode> {
        let device = evdev::Device::open(node)?;
        device.set_nonblocking(true)?;
        Ok(EvdevNode { device })
    }
}

/// An open evdev node, read nonblocking and never grabbed.
pub(crate) struct EvdevNode {
    device: evdev::Device,
}

impl InputNode for EvdevNode {
    fn held_keys(&self) -> io::Result<Vec<u16>> {
        Ok(self
            .device
            .get_key_state()?
            .iter()
            .map(evdev::KeyCode::code)
            .collect())
    }

    fn abs_info(&self, code: u16) -> io::Result<AxisState> {
        let state = self.device.get_abs_state()?;
        Ok(state.get(usize::from(code)).map_or(
            AxisState {
                range: AxisRange { min: 0, max: 0 },
                value: 0,
            },
            |info| AxisState {
                range: AxisRange {
                    min: info.minimum,
                    max: info.maximum,
                },
                value: info.value,
            },
        ))
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<bool> {
        let timeout = Timespec::try_from(timeout).map_err(io::Error::other)?;
        let mut fds = [PollFd::new(&self.device, PollFlags::IN)];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(ready) => Ok(ready > 0),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn read(&mut self) -> io::Result<Vec<InputEvent>> {
        Ok(self
            .device
            .fetch_events()?
            .map(|event| InputEvent {
                event_type: event.event_type().0,
                code: event.code(),
                value: event.value(),
                at: event
                    .timestamp()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
