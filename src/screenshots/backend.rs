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
use std::thread;
use std::time::Duration;

use jiff::tz::TimeZone;
use x52::command::ControlTransfer;
use x52::device::DeviceType;
use x52::input::AxisState;
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
        Ok(ModeOneNode)
    }
}

/// A node that holds the mode 1 key and never sends an event.
#[derive(Debug)]
pub(crate) struct ModeOneNode;

impl InputNode for ModeOneNode {
    fn held_keys(&self) -> io::Result<Vec<u16>> {
        Ok(vec![MODE_1_KEY])
    }

    fn abs_info(&self, _code: u16) -> io::Result<AxisState> {
        Ok(AxisState {
            range: x52::input::AxisRange { min: 0, max: 0 },
            value: 0,
        })
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<bool> {
        thread::sleep(timeout);
        Ok(false)
    }

    fn read(&mut self) -> io::Result<Vec<InputEvent>> {
        Ok(Vec::new())
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
