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

//! The device service: the threads that own the joystick (docs/architecture.md, "Threads and data
//! flow").
//!
//! [`DeviceService::start`] spawns the device watch, the writer and the evdev reader. The main
//! thread pushes the desired state through the returned handle and receives [`DeviceEvent`]s;
//! nothing here touches a `GObject`.

pub(crate) mod access;
pub(crate) mod id;
pub(crate) mod monotonic;
pub(crate) mod reader;
pub(crate) mod status;
pub(crate) mod usb;
pub(crate) mod watch;
pub(crate) mod writer;
pub(crate) mod x52d;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use jiff::tz::TimeZone;
use x52::clock::Clock;
use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::DeviceState;

use self::access::AccessState;
use self::monotonic::SystemMonotonic;
use self::reader::{Reader, ReaderEvent, ReaderMessage, SysfsEvdev};
use self::usb::NusbOpener;
use self::watch::{SelectionChange, ShutdownTrigger, WatchEvent};
use self::writer::{Writer, WriterEvent, WriterMessage};

/// What the device service reports to the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceEvent {
    /// The type of the selected device, or `None` once it is gone.
    DeviceType(Option<DeviceType>),
    /// The access state changed.
    Access(AccessState),
    /// The selected device's evdev node was opened, or closed (`None`).
    InputNode(Option<PathBuf>),
    /// The active mode: the initial one, then every debounced change.
    Mode(Mode),
    /// libx52's `x52d` started or stopped running.
    X52dRunning(bool),
    /// Something failed; already logged by the thread that reports it.
    Error(String),
}

impl From<WriterEvent> for DeviceEvent {
    fn from(event: WriterEvent) -> Self {
        match event {
            WriterEvent::Access(access) => Self::Access(access),
            WriterEvent::Error(error) => Self::Error(writer::error_chain(&error)),
        }
    }
}

impl From<ReaderEvent> for DeviceEvent {
    fn from(event: ReaderEvent) -> Self {
        match event {
            ReaderEvent::NodeOpened { node, .. } => Self::InputNode(Some(node)),
            ReaderEvent::NodeClosed { .. } => Self::InputNode(None),
            ReaderEvent::Mode(mode) => Self::Mode(mode),
        }
    }
}

/// The handle of the running device service.
///
/// Dropping it shuts the service down, as [`DeviceService::shutdown`] does.
#[derive(Debug)]
pub(crate) struct DeviceService {
    writer: Sender<WriterMessage>,
    reader: Sender<ReaderMessage>,
    /// The watch's shutdown trigger and every thread, until shutdown.
    running: Option<(ShutdownTrigger, Vec<JoinHandle<()>>)>,
}

impl DeviceService {
    /// Spawns the device watch, the writer and the evdev reader, with `clock` as the writer's
    /// time source; returns the handle and the receiver of the service's events.
    ///
    /// # Errors
    ///
    /// The I/O error of spawning a thread.
    pub(crate) fn start<C: Clock + 'static>(
        clock: C,
    ) -> io::Result<(Self, async_channel::Receiver<DeviceEvent>)> {
        let (events, received) = async_channel::unbounded();
        let (writer, writer_messages) = mpsc::channel();
        let (reader, reader_messages) = mpsc::channel();
        let (trigger, signal) = watch::shutdown_signal();
        let mut threads = Vec::new();

        let writer_events = events.clone();
        threads.push(spawn("device-writer", move || {
            Writer::new(
                NusbOpener,
                clock,
                SystemMonotonic::new(),
                TimeZone::system,
                writer_events,
            )
            .run(&writer_messages);
        })?);

        let reader_events = events.clone();
        threads.push(spawn("evdev-reader", move || {
            Reader::new(SysfsEvdev, SystemMonotonic::new(), reader_events).run(&reader_messages);
        })?);

        let to_writer = writer.clone();
        let to_reader = reader.clone();
        threads.push(spawn("device-watch", move || {
            watch::run(&signal, |event| match event {
                WatchEvent::Selection(change) => {
                    // A send fails only once that thread has stopped, at shutdown.
                    let _ = to_writer.send(change.to_writer());
                    let _ = to_reader.send(change.to_reader());
                    let device_type = match change {
                        SelectionChange::Selected(device) => Some(device.device_type),
                        SelectionChange::Gone(_) => None,
                    };
                    let _ = events.try_send(DeviceEvent::DeviceType(device_type));
                }
                WatchEvent::X52dRunning(running) => {
                    let _ = events.try_send(DeviceEvent::X52dRunning(running));
                }
            });
        })?);

        let service = Self {
            writer,
            reader,
            running: Some((trigger, threads)),
        };
        Ok((service, received))
    }

    /// Sets the desired device state; the writer sends what differs from what the device shows.
    pub(crate) fn set_state(&self, state: DeviceState) {
        let _ = self.writer.send(WriterMessage::SetState(state));
    }

    /// Tries to open the device again at once, after a denied permission.
    pub(crate) fn retry(&self) {
        let _ = self.writer.send(WriterMessage::Retry);
    }

    /// Signals every thread to stop and returns at once; the threads are joined on a thread of
    /// their own, never on the caller's.
    pub(crate) fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let Some((trigger, threads)) = self.running.take() else {
            return;
        };
        let _ = self.writer.send(WriterMessage::Shutdown);
        let _ = self.reader.send(ReaderMessage::Shutdown);
        drop(trigger);
        let joined = thread::Builder::new()
            .name("device-join".into())
            .spawn(move || {
                for thread in threads {
                    if thread.join().is_err() {
                        tracing::error!("A device thread panicked");
                    }
                }
            });
        if let Err(error) = joined {
            tracing::warn!("Cannot join the device threads: {error}");
        }
    }
}

impl Drop for DeviceService {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Spawns a named thread.
fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.into()).spawn(body)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use x52::input::Mode;

    use super::*;
    use crate::device::id::Generation;
    use crate::device::usb::OpenError;
    use crate::device::writer::WriterError;

    #[test]
    fn writer_events_become_device_events() {
        assert_eq!(
            DeviceEvent::from(WriterEvent::Access(AccessState::NoPermission)),
            DeviceEvent::Access(AccessState::NoPermission)
        );
        assert_eq!(
            DeviceEvent::from(WriterEvent::Error(WriterError::Open(OpenError::NotListed))),
            DeviceEvent::Error("cannot open the device: the device is no longer listed".into())
        );
    }

    #[test]
    fn reader_events_become_device_events() {
        let node = PathBuf::from("/dev/input/event20");
        assert_eq!(
            DeviceEvent::from(ReaderEvent::NodeOpened {
                generation: Generation(1),
                node: node.clone(),
            }),
            DeviceEvent::InputNode(Some(node))
        );
        assert_eq!(
            DeviceEvent::from(ReaderEvent::NodeClosed {
                generation: Generation(1)
            }),
            DeviceEvent::InputNode(None)
        );
        assert_eq!(
            DeviceEvent::from(ReaderEvent::Mode(Mode::Two)),
            DeviceEvent::Mode(Mode::Two)
        );
    }
}
