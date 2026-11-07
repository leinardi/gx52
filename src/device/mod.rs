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
pub(crate) mod check;
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
use x52::command::Command;
use x52::input::Mode;
use x52::state::DeviceState;
use x52::transport::Transport;

use self::access::AccessState;
use self::id::{DeviceId, Generation};
use self::monotonic::SystemMonotonic;
use self::reader::{EvdevSource, Reader, ReaderEvent, ReaderMessage, SysfsEvdev};
use self::usb::{NusbOpener, Opener};
use self::watch::{SelectionChange, ShutdownSignal, ShutdownTrigger, WatchEvent};
use self::writer::{Writer, WriterEvent, WriterMessage};

/// Where the service's threads get the device from: the real system, or fakes in the tests.
pub(crate) trait Backend: Send + 'static {
    /// Opens the selected device for the writer.
    type Opener: Opener + Send + 'static;
    /// Lists and opens evdev nodes for the reader.
    type Evdev: EvdevSource + Send + 'static;

    /// The writer's opener.
    fn opener(&mut self) -> Self::Opener;

    /// The reader's evdev source.
    fn evdev(&mut self) -> Self::Evdev;

    /// The system time zone the writer's clocks follow.
    fn local_zone(&self) -> fn() -> TimeZone;

    /// Runs the device watch on its thread until `shutdown`, passing every event to `notify`.
    fn watch(self, shutdown: &ShutdownSignal, notify: impl FnMut(WatchEvent));
}

/// The [`Backend`] of the real system: `nusb`, sysfs and `/dev/input`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemBackend;

impl Backend for SystemBackend {
    type Opener = NusbOpener;
    type Evdev = SysfsEvdev;

    fn opener(&mut self) -> NusbOpener {
        NusbOpener
    }

    fn evdev(&mut self) -> SysfsEvdev {
        SysfsEvdev
    }

    fn local_zone(&self) -> fn() -> TimeZone {
        TimeZone::system
    }

    fn watch(self, shutdown: &ShutdownSignal, notify: impl FnMut(WatchEvent)) {
        watch::run(shutdown, notify);
    }
}

/// What the device service reports to the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeviceEvent {
    /// A device, or a new attachment of it, is selected.
    Selected(DeviceId),
    /// The selection of that generation is gone.
    Gone(Generation),
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
    /// The writer sent this command successfully.
    Sent(Command),
}

impl From<WriterEvent> for DeviceEvent {
    fn from(event: WriterEvent) -> Self {
        match event {
            WriterEvent::Access(access) => Self::Access(access),
            WriterEvent::Error(error) => Self::Error(writer::error_chain(&error)),
            WriterEvent::Sent(command) => Self::Sent(command),
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
    /// Spawns the device watch, the writer and the evdev reader on the real system, with `clock`
    /// as the writer's time source; returns the handle and the receiver of the service's events.
    ///
    /// # Errors
    ///
    /// The I/O error of spawning a thread.
    pub(crate) fn start<C: Clock + 'static>(
        clock: C,
    ) -> io::Result<(Self, async_channel::Receiver<DeviceEvent>)> {
        Self::start_with(clock, SystemBackend)
    }

    /// [`DeviceService::start`] over `backend`.
    ///
    /// # Errors
    ///
    /// The I/O error of spawning a thread.
    pub(crate) fn start_with<C, B>(
        clock: C,
        mut backend: B,
    ) -> io::Result<(Self, async_channel::Receiver<DeviceEvent>)>
    where
        C: Clock + 'static,
        B: Backend,
        <<B::Opener as Opener>::Transport as Transport>::Error: Send + Sync,
    {
        let (events, received) = async_channel::unbounded();
        let (writer, writer_messages) = mpsc::channel();
        let (reader, reader_messages) = mpsc::channel();
        let (trigger, signal) = watch::shutdown_signal();
        let mut threads = Vec::new();

        let opener = backend.opener();
        let evdev = backend.evdev();
        let local_zone = backend.local_zone();
        let writer_events = events.clone();
        threads.push(spawn("device-writer", move || {
            Writer::new(
                opener,
                clock,
                SystemMonotonic::new(),
                local_zone,
                writer_events,
            )
            .run(&writer_messages);
        })?);

        let reader_events = events.clone();
        threads.push(spawn("evdev-reader", move || {
            Reader::new(evdev, SystemMonotonic::new(), reader_events).run(&reader_messages);
        })?);

        let to_writer = writer.clone();
        let to_reader = reader.clone();
        threads.push(spawn("device-watch", move || {
            backend.watch(&signal, |event| match event {
                WatchEvent::Selection(change) => {
                    // A send fails only once that thread has stopped, at shutdown.
                    let _ = to_writer.send(change.to_writer());
                    let _ = to_reader.send(change.to_reader());
                    let _ = events.try_send(match change {
                        SelectionChange::Selected(device) => DeviceEvent::Selected(device),
                        SelectionChange::Gone(generation) => DeviceEvent::Gone(generation),
                    });
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
