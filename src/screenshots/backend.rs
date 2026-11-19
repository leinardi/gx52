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

//! The X52 Pro the screenshots show: always connected, open, and in mode 1. Nothing here touches
//! a device; every transfer and pointer event is dropped.

use std::convert::Infallible;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use jiff::tz::TimeZone;
use x52::command::ControlTransfer;
use x52::device::DeviceType;
use x52::input::{AxisRange, AxisState};
use x52::transport::Transport;

use crate::device::Backend;
use crate::device::id::{Attachment, DeviceId, Generation};
use crate::device::mouse::RawEvent;
use crate::device::mouse_loop::{Pointer, PointerFactory};
use crate::device::reader::{EvdevSource, InputEvent, InputNode, NodeEntry};
use crate::device::usb::{OpenError, Opener};
use crate::device::watch::{SelectionChange, ShutdownSignal, WatchEvent};

/// The sysfs directory of the fake joystick.
const USB: &str = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4";

/// `BTN_TRIGGER_HAPPY12`: the mode selector in mode 1 on the X52 Pro (x52's input map).
pub(super) const MODE_1_KEY: u16 = 715;

/// How long the shutdown signal is waited for at a time.
const SHUTDOWN_POLL: Duration = Duration::from_secs(1);

/// How long the node waits for the pose at a time, so it shows soon after it is asked for.
const POSE_POLL: Duration = Duration::from_millis(50);

/// The evdev event types the pose uses.
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;

/// `ABS_HAT0Y`: POV 1, up to down.
const ABS_HAT0Y: u16 = 0x11;

/// The buttons the input tester shot holds, by evdev code (x52's X52 Pro map): the trigger's
/// first stage, A, toggle 1 up and POV 2 right.
const POSE_KEYS: [u16; 4] = [288, 290, 296, 708];

/// Set by the input tester shot, the last: from then on the node reports the pose. The other
/// shots see the joystick at rest, so what they show does not depend on it.
static POSE: AtomicBool = AtomicBool::new(false);

/// Puts the joystick in the pose of the input tester shot.
pub(super) fn show_pose() {
    POSE.store(true, Ordering::SeqCst);
}

/// The range of the X52 Pro's absolute axis `code`, and its value at rest and in the pose; `None`
/// for a code the joystick does not have. POV 1 rests at 0 and is moved by an event instead, as
/// the reader takes it from the held inputs.
const fn axis(code: u16) -> Option<(AxisRange, i32, i32)> {
    let (min, max, rest, pose) = match code {
        // The stick, a little forward and to the right.
        0x00 => (0, 1023, 512, 700),
        0x01 => (0, 1023, 512, 360),
        // The throttle, two thirds forward (the axis reads 255 at the back).
        0x02 => (0, 255, 255, 85),
        // The rotaries and the slider.
        0x03 => (0, 255, 128, 200),
        0x04 => (0, 255, 128, 60),
        0x05 => (0, 1023, 512, 600),
        0x06 => (0, 255, 128, 170),
        0x10 | 0x11 => (-1, 1, 0, 0),
        // The ministick, up and to the right.
        0x28 => (0, 15, 8, 12),
        0x29 => (0, 15, 8, 4),
        _ => return None,
    };
    Some((AxisRange { min, max }, rest, pose))
}

/// The fake joystick: see the module documentation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScreenshotBackend {
    /// The local time zone the writer's clocks follow.
    pub(crate) local_zone: fn() -> TimeZone,
}

/// The one selection the watch reports.
fn device() -> DeviceId {
    DeviceId {
        device_type: DeviceType::X52Pro,
        sysfs_path: PathBuf::from(USB),
        attachment: Attachment { bus: 1, address: 4 },
        generation: Generation(1),
    }
}

impl Backend for ScreenshotBackend {
    type Opener = NullOpener;
    type Evdev = ModeOneEvdev;
    type Pointers = NullPointers;

    fn opener(&mut self) -> NullOpener {
        NullOpener
    }

    fn evdev(&mut self) -> ModeOneEvdev {
        ModeOneEvdev
    }

    fn pointers(&mut self) -> NullPointers {
        NullPointers
    }

    fn local_zone(&self) -> fn() -> TimeZone {
        self.local_zone
    }

    fn watch(self, shutdown: &ShutdownSignal, mut notify: impl FnMut(WatchEvent)) {
        notify(WatchEvent::X52dRunning(false));
        notify(WatchEvent::Selection(SelectionChange::Selected(device())));
        while !shutdown.wait_timeout(SHUTDOWN_POLL) {}
    }
}

/// Opens the fake joystick: always allowed.
#[derive(Debug)]
pub(crate) struct NullOpener;

impl Opener for NullOpener {
    type Transport = NullTransport;

    fn open(&mut self, _: &DeviceId) -> Result<NullTransport, OpenError> {
        Ok(NullTransport)
    }
}

/// Accepts every transfer and sends none.
#[derive(Debug)]
pub(crate) struct NullTransport;

impl Transport for NullTransport {
    type Error = Infallible;

    fn control_out(&mut self, _: ControlTransfer) -> Result<(), Infallible> {
        Ok(())
    }
}

/// The fake joystick's one evdev node.
#[derive(Debug)]
pub(crate) struct ModeOneEvdev;

impl EvdevSource for ModeOneEvdev {
    type Node = ModeOneNode;

    fn nodes(&mut self) -> Vec<NodeEntry> {
        vec![NodeEntry {
            node: PathBuf::from("/dev/input/event20"),
            device: PathBuf::from(format!("{USB}/1-4:1.0/input/input20")),
        }]
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn open(&mut self, _: &Path) -> io::Result<ModeOneNode> {
        Ok(ModeOneNode { posed: false })
    }
}

/// A node that holds the mode 1 key, rests until [`show_pose`], and then sends the pose once.
#[derive(Debug)]
pub(crate) struct ModeOneNode {
    /// Whether the pose's events were read.
    posed: bool,
}

impl ModeOneNode {
    /// Whether the pose is asked for and its events not read yet.
    fn pose_due(&self) -> bool {
        !self.posed && POSE.load(Ordering::SeqCst)
    }
}

impl InputNode for ModeOneNode {
    fn held_keys(&self) -> io::Result<Vec<u16>> {
        Ok(vec![MODE_1_KEY])
    }

    fn abs_info(&self, code: u16) -> io::Result<AxisState> {
        let posed = POSE.load(Ordering::SeqCst);
        Ok(axis(code).map_or(
            AxisState {
                range: AxisRange { min: 0, max: 0 },
                value: 0,
            },
            |(range, rest, pose)| AxisState {
                range,
                value: if posed { pose } else { rest },
            },
        ))
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<bool> {
        let mut left = timeout;
        while !left.is_zero() {
            if self.pose_due() {
                return Ok(true);
            }
            let step = left.min(POSE_POLL);
            thread::sleep(step);
            left -= step;
        }
        Ok(self.pose_due())
    }

    fn read(&mut self) -> io::Result<Vec<InputEvent>> {
        if !self.pose_due() {
            return Ok(Vec::new());
        }
        self.posed = true;
        let event = |event_type, code, value| InputEvent {
            event_type,
            code,
            value,
            at: Duration::ZERO,
        };
        let mut events: Vec<InputEvent> = POSE_KEYS
            .iter()
            .map(|code| event(EV_KEY, *code, 1))
            .collect();
        // The other axes come from `abs_info` once the tester listens; an event per axis would
        // reach the virtual mouse too, which needs nothing.
        events.push(event(EV_ABS, ABS_HAT0Y, -1));
        events.push(event(EV_SYN, 0, 0));
        Ok(events)
    }
}

/// Creates pointers that drop their events, so the virtual mouse shows as working.
#[derive(Debug)]
pub(crate) struct NullPointers;

/// A pointer that moves nothing.
#[derive(Debug)]
pub(crate) struct NullPointer;

impl Pointer for NullPointer {
    fn emit(&mut self, _events: &[RawEvent]) -> io::Result<()> {
        Ok(())
    }
}

impl PointerFactory for NullPointers {
    type Pointer = NullPointer;

    fn create(&mut self) -> io::Result<NullPointer> {
        Ok(NullPointer)
    }
}
