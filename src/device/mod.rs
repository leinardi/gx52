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
pub(crate) mod names;
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
pub(crate) use self::reader::HeldChange;
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
    /// The evdev node of the selection `generation` was opened, or closed (`None`).
    InputNode {
        /// The selection it belongs to.
        generation: Generation,
        /// The node, `None` once closed.
        node: Option<PathBuf>,
    },
    /// The active mode of the selection `generation`: the initial one, then every debounced
    /// change.
    Mode {
        /// The selection it belongs to.
        generation: Generation,
        /// The mode.
        mode: Mode,
    },
    /// The held inputs of the selection `generation` changed.
    Held {
        /// The selection it belongs to.
        generation: Generation,
        /// What changed.
        change: HeldChange,
    },
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
            ReaderEvent::NodeOpened { generation, node } => Self::InputNode {
                generation,
                node: Some(node),
            },
            ReaderEvent::NodeClosed { generation } => Self::InputNode {
                generation,
                node: None,
            },
            ReaderEvent::Mode { generation, mode } => Self::Mode { generation, mode },
            ReaderEvent::Held { generation, change } => Self::Held { generation, change },
        }
    }
}

/// The selection the main thread follows, to drop the reader events of an earlier one.
///
/// The reader can still be reporting on the previous selection when the next `Selected` reaches
/// the main thread (a mode change in flight, its node closing): those events carry the earlier
/// generation and must not apply to the new device.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurrentSelection(Option<Generation>);

impl CurrentSelection {
    /// Follows `event` and returns whether it belongs to the current selection: `false` for a
    /// node, mode or held-input event of another generation and for a `Gone` of a selection that is not the
    /// current one.
    pub(crate) fn accept(&mut self, event: &DeviceEvent) -> bool {
        match event {
            DeviceEvent::Selected(device) => {
                self.0 = Some(device.generation);
                true
            }
            DeviceEvent::Gone(generation) => {
                let current = self.0 == Some(*generation);
                if current {
                    self.0 = None;
                }
                current
            }
            DeviceEvent::InputNode { generation, .. }
            | DeviceEvent::Mode { generation, .. }
            | DeviceEvent::Held { generation, .. } => self.0 == Some(*generation),
            DeviceEvent::Access(_)
            | DeviceEvent::X52dRunning(_)
            | DeviceEvent::Error(_)
            | DeviceEvent::Sent(_) => true,
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
                    // The main thread hears of the change first: the reader and the writer report
                    // through the same FIFO channel only after they hear of it, so every event of
                    // a selection (the reader's initial mode above all) arrives after its
                    // `Selected` and is never discarded as belonging to the previous one.
                    let _ = events.try_send(match &change {
                        SelectionChange::Selected(device) => DeviceEvent::Selected(device.clone()),
                        SelectionChange::Gone(generation) => DeviceEvent::Gone(*generation),
                    });
                    // A send fails only once that thread has stopped, at shutdown.
                    let _ = to_writer.send(change.to_writer());
                    let _ = to_reader.send(change.to_reader());
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
            DeviceEvent::InputNode {
                generation: Generation(1),
                node: Some(node)
            }
        );
        assert_eq!(
            DeviceEvent::from(ReaderEvent::NodeClosed {
                generation: Generation(1)
            }),
            DeviceEvent::InputNode {
                generation: Generation(1),
                node: None
            }
        );
        assert_eq!(
            DeviceEvent::from(ReaderEvent::Mode {
                generation: Generation(1),
                mode: Mode::Two
            }),
            DeviceEvent::Mode {
                generation: Generation(1),
                mode: Mode::Two
            }
        );
    }

    fn selected(generation: u64) -> DeviceEvent {
        DeviceEvent::Selected(DeviceId {
            device_type: x52::device::DeviceType::X52Pro,
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-1"),
            attachment: id::Attachment {
                bus: 1,
                address: u8::try_from(generation).unwrap(),
            },
            generation: Generation(generation),
        })
    }

    fn mode(generation: u64, mode: Mode) -> DeviceEvent {
        DeviceEvent::Mode {
            generation: Generation(generation),
            mode,
        }
    }

    #[test]
    fn a_late_event_of_the_previous_selection_is_dropped() {
        let mut current = CurrentSelection::default();
        assert!(
            !current.accept(&mode(1, Mode::Three)),
            "nothing selected yet"
        );
        assert!(current.accept(&selected(1)));
        assert!(current.accept(&mode(1, Mode::Three)));
        // The next device is selected while the reader still reports on the first one.
        assert!(current.accept(&DeviceEvent::Gone(Generation(1))));
        assert!(current.accept(&selected(2)));
        assert!(!current.accept(&mode(1, Mode::Two)));
        assert!(!current.accept(&DeviceEvent::InputNode {
            generation: Generation(1),
            node: None,
        }));
        assert!(!current.accept(&DeviceEvent::Gone(Generation(1))));
        assert!(current.accept(&mode(2, Mode::One)));
        assert!(current.accept(&DeviceEvent::InputNode {
            generation: Generation(2),
            node: None,
        }));
        assert!(current.accept(&DeviceEvent::Access(AccessState::NoPermission)));
        assert!(current.accept(&DeviceEvent::Gone(Generation(2))));
        assert!(!current.accept(&mode(2, Mode::One)), "gone");
    }
}
