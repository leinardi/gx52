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

//! The device watch: which supported device is selected, and when it goes away or is attached
//! again (docs/architecture.md, "Threads and data flow", "Device watch").
//!
//! One component, two sources (S4): `nusb::watch_devices` on the host, where every hotplug event
//! leads to a re-list (and every [`x52d::CHECK_INTERVAL`], for the events the kernel drops), and
//! polling `nusb::list_devices` every [`POLL_INTERVAL`] in the Flatpak sandbox, where the hotplug
//! stream never yields. Devices are matched by vendor and product ID with
//! [`DeviceType::from_ids`], never by address, and chosen by their sysfs path.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use async_io::Timer;
use futures_lite::{Stream, StreamExt, future};
use nusb::MaybeFuture;
use nusb::hotplug::HotplugEvent;
use x52::device::DeviceType;

use super::id::{Attachment, DeviceId, Generation};
use super::reader::ReaderMessage;
use super::writer::WriterMessage;
use super::x52d::{self, X52dWatch};

/// How often the sandbox polls the device list.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// The file that exists only inside a Flatpak sandbox.
const FLATPAK_INFO: &str = "/.flatpak-info";

/// A change of the selection, for the writer, the reader and the main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectionChange {
    /// A device, or a new attachment of the selected one, is selected.
    Selected(DeviceId),
    /// The selection of that generation is gone.
    Gone(Generation),
}

impl SelectionChange {
    /// The message that tells the writer about this change.
    pub(crate) fn to_writer(&self) -> WriterMessage {
        match self {
            Self::Selected(device) => WriterMessage::Selected(device.clone()),
            Self::Gone(generation) => WriterMessage::Gone(*generation),
        }
    }

    /// The message that tells the evdev reader about this change.
    pub(crate) fn to_reader(&self) -> ReaderMessage {
        match self {
            Self::Selected(device) => ReaderMessage::Selected(device.clone()),
            Self::Gone(generation) => ReaderMessage::Gone(*generation),
        }
    }
}

/// What the watch reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WatchEvent {
    /// The selection changed.
    Selection(SelectionChange),
    /// libx52's `x52d` started or stopped running (reported at start, then on change).
    X52dRunning(bool),
}

/// One device of a USB device list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsbEntry<K> {
    /// What a host `Disconnected` event names this attachment by (`nusb::DeviceId`).
    pub(crate) key: K,
    /// The USB vendor ID.
    pub(crate) vendor_id: u16,
    /// The USB product ID.
    pub(crate) product_id: u16,
    /// The USB device's sysfs directory.
    pub(crate) sysfs_path: PathBuf,
    /// The attachment.
    pub(crate) attachment: Attachment,
}

/// The selection over successive device lists and hotplug events; pure, so it is tested with
/// fake lists.
#[derive(Debug)]
pub(crate) struct Selector<K> {
    /// The selected device and the key of its attachment.
    selected: Option<(DeviceId, K)>,
    /// The generation of the latest selection.
    last_generation: u64,
}

impl<K> Default for Selector<K> {
    fn default() -> Self {
        Self {
            selected: None,
            last_generation: 0,
        }
    }
}

impl<K: Clone + PartialEq> Selector<K> {
    /// The selected device.
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<&DeviceId> {
        self.selected.as_ref().map(|(device, _)| device)
    }

    /// Applies a complete device list.
    ///
    /// The selection is kept while its sysfs path is listed with the same attachment, whatever
    /// the order of the list and whatever else appears. The same path with another attachment is a
    /// reattachment: `Gone` for the old generation, then `Selected` with a new one. When the path
    /// is no longer listed, or when nothing is selected, the first supported device in sysfs-path
    /// order is selected with a new generation.
    pub(crate) fn update(&mut self, entries: &[UsbEntry<K>]) -> Vec<SelectionChange> {
        let mut matching: Vec<(DeviceType, &UsbEntry<K>)> = entries
            .iter()
            .filter_map(|entry| {
                DeviceType::from_ids(entry.vendor_id, entry.product_id).map(|kind| (kind, entry))
            })
            .collect();
        matching.sort_by(|(_, left), (_, right)| left.sysfs_path.cmp(&right.sysfs_path));

        let mut changes = Vec::new();
        if let Some((device, _)) = &self.selected {
            let current = matching
                .iter()
                .find(|(_, entry)| entry.sysfs_path == device.sysfs_path);
            match current {
                Some((kind, entry))
                    if entry.attachment == device.attachment && *kind == device.device_type =>
                {
                    return changes;
                }
                Some((kind, entry)) => {
                    changes.push(SelectionChange::Gone(device.generation));
                    changes.push(self.select(*kind, entry));
                    return changes;
                }
                None => {
                    changes.push(SelectionChange::Gone(device.generation));
                    self.selected = None;
                }
            }
        }
        if let Some((kind, entry)) = matching.first() {
            changes.push(self.select(*kind, entry));
        }
        changes
    }

    /// Applies a host `Disconnected` event: `Gone` when it names the selected attachment.
    pub(crate) fn disconnected(&mut self, key: &K) -> Vec<SelectionChange> {
        match &self.selected {
            Some((device, selected)) if selected == key => {
                let gone = SelectionChange::Gone(device.generation);
                self.selected = None;
                vec![gone]
            }
            Some(_) | None => Vec::new(),
        }
    }

    fn select(&mut self, device_type: DeviceType, entry: &UsbEntry<K>) -> SelectionChange {
        self.last_generation += 1;
        let device = DeviceId {
            device_type,
            sysfs_path: entry.sysfs_path.clone(),
            attachment: entry.attachment,
            generation: Generation(self.last_generation),
        };
        self.selected = Some((device.clone(), entry.key.clone()));
        SelectionChange::Selected(device)
    }
}

/// Lists the USB devices; the watch is generic over it so the tests can script the lists.
pub(crate) trait Lister {
    /// What a host `Disconnected` event names an attachment by.
    type Key: Clone + PartialEq;

    /// Every USB device now attached.
    ///
    /// # Errors
    ///
    /// The I/O error of the listing.
    fn list(&mut self) -> io::Result<Vec<UsbEntry<Self::Key>>>;
}

/// The [`Lister`] of the real device list, through `nusb`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NusbLister;

impl Lister for NusbLister {
    type Key = nusb::DeviceId;

    fn list(&mut self) -> io::Result<Vec<UsbEntry<nusb::DeviceId>>> {
        let devices = nusb::list_devices().wait()?;
        Ok(devices
            .map(|info| UsbEntry {
                key: info.id(),
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
                sysfs_path: info.sysfs_path().to_path_buf(),
                attachment: Attachment::of(&info),
            })
            .collect())
    }
}

/// A host hotplug event, reduced to what the watch needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hotplug<K> {
    /// A device was attached; the watch re-lists.
    Connected,
    /// The attachment `K` went away; it carries no vendor or product ID, so the watch re-lists.
    Disconnected(K),
}

/// Creates the two ends of a shutdown signal; dropping the trigger signals it.
pub(crate) fn shutdown_signal() -> (ShutdownTrigger, ShutdownSignal) {
    let (blocking_sender, blocking) = mpsc::channel();
    let (notify_sender, notify) = async_channel::bounded(1);
    (
        ShutdownTrigger {
            _blocking: blocking_sender,
            _notify: notify_sender,
        },
        ShutdownSignal { blocking, notify },
    )
}

/// Signals a [`ShutdownSignal`] when dropped.
#[derive(Debug)]
pub(crate) struct ShutdownTrigger {
    _blocking: mpsc::Sender<()>,
    _notify: async_channel::Sender<()>,
}

/// What the watch waits on: a blocking wait with a timeout for polling, a future for the hotplug
/// stream.
#[derive(Debug)]
pub(crate) struct ShutdownSignal {
    blocking: mpsc::Receiver<()>,
    notify: async_channel::Receiver<()>,
}

impl ShutdownSignal {
    /// Waits up to `timeout`; true once shutdown is signalled.
    pub(crate) fn wait_timeout(&self, timeout: Duration) -> bool {
        match self.blocking.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        }
    }

    /// Resolves once shutdown is signalled.
    async fn signalled(&self) {
        // The trigger never sends: the channel closing is the signal.
        let _ = self.notify.recv().await;
    }
}

/// Whether GX52 runs inside a Flatpak sandbox.
pub(crate) fn is_sandboxed() -> bool {
    Path::new(FLATPAK_INFO).exists()
}

/// Runs the watch until shutdown, with the source that works where the app runs, and passes
/// every selection change and every change of `x52d`'s state to `notify`.
pub(crate) fn run(shutdown: &ShutdownSignal, notify: impl FnMut(WatchEvent)) {
    if is_sandboxed() {
        tracing::info!("Sandboxed: polling the USB device list");
        run_polling(NusbLister, x52d::is_running, shutdown, notify);
        return;
    }
    // The stream is created before the first list, so no event between the two is lost.
    match nusb::watch_devices() {
        Ok(watch) => {
            let events = watch.map(|event| match event {
                HotplugEvent::Connected(_) => Hotplug::Connected,
                HotplugEvent::Disconnected(id) => Hotplug::Disconnected(id),
            });
            run_hotplug(NusbLister, events, x52d::is_running, shutdown, notify);
        }
        Err(error) => {
            tracing::warn!("Cannot watch USB hotplug events, polling instead: {error}");
            run_polling(NusbLister, x52d::is_running, shutdown, notify);
        }
    }
}

/// The host source: a re-list at start and after every hotplug event, and an `x52d` check at
/// start, until shutdown. Every [`x52d::CHECK_INTERVAL`] it checks `x52d` and re-lists too: the
/// kernel drops hotplug events when the netlink socket overflows (`ENOBUFS`, seen on resume from
/// suspend), and `nusb` only logs that, so a lost event is made up for within that interval.
pub(crate) fn run_hotplug<L, S>(
    lister: L,
    mut events: S,
    check_x52d: impl FnMut() -> bool,
    shutdown: &ShutdownSignal,
    mut notify: impl FnMut(WatchEvent),
) where
    L: Lister,
    S: Stream<Item = Hotplug<L::Key>> + Unpin,
{
    /// What ended the wait.
    enum Next<K> {
        Event(Hotplug<K>),
        StreamEnded,
        Tick,
        Shutdown,
    }

    let mut watch = Listing::new(lister, check_x52d);
    watch.check_x52d(&mut notify);
    watch.relist(&mut notify);
    let mut ticks = Timer::interval(x52d::CHECK_INTERVAL);
    loop {
        let next = future::block_on(future::or(
            async { events.next().await.map_or(Next::StreamEnded, Next::Event) },
            future::or(
                async {
                    ticks.next().await;
                    Next::Tick
                },
                async {
                    shutdown.signalled().await;
                    Next::Shutdown
                },
            ),
        ));
        match next {
            Next::Event(Hotplug::Disconnected(key)) => watch.disconnected(key, &mut notify),
            Next::Event(Hotplug::Connected) => {}
            Next::Tick => watch.check_x52d(&mut notify),
            Next::StreamEnded => {
                tracing::warn!("The USB hotplug stream ended; the device watch stops");
                return;
            }
            Next::Shutdown => return,
        }
        watch.relist(&mut notify);
    }
}

/// The sandbox source: a re-list every [`POLL_INTERVAL`] and an `x52d` check every
/// [`x52d::CHECK_INTERVAL`], until shutdown.
pub(crate) fn run_polling<L: Lister>(
    lister: L,
    check_x52d: impl FnMut() -> bool,
    shutdown: &ShutdownSignal,
    mut notify: impl FnMut(WatchEvent),
) {
    let polls_per_check = x52d::CHECK_INTERVAL.as_millis() / POLL_INTERVAL.as_millis();
    let mut watch = Listing::new(lister, check_x52d);
    let mut polls = 0;
    loop {
        if polls % polls_per_check == 0 {
            watch.check_x52d(&mut notify);
        }
        polls += 1;
        watch.relist(&mut notify);
        if shutdown.wait_timeout(POLL_INTERVAL) {
            return;
        }
    }
}

/// A lister, the selection over its lists, and the `x52d` check.
struct Listing<L: Lister, C> {
    lister: L,
    check_x52d: C,
    x52d: X52dWatch,
    selector: Selector<L::Key>,
    /// Whether the last listing failed, so a failure is logged once until a listing succeeds.
    list_failed: bool,
    /// Attachments a host `Disconnected` named. The sysfs directory can outlive the event for a
    /// moment, so they are left out of the listings until a listing no longer contains them.
    departed: Vec<L::Key>,
}

impl<L: Lister, C: FnMut() -> bool> Listing<L, C> {
    fn new(lister: L, check_x52d: C) -> Self {
        Self {
            lister,
            check_x52d,
            x52d: X52dWatch::default(),
            selector: Selector::default(),
            list_failed: false,
            departed: Vec::new(),
        }
    }

    /// Checks whether `x52d` runs and reports a change.
    fn check_x52d(&mut self, notify: &mut impl FnMut(WatchEvent)) {
        if let Some(running) = self.x52d.update((self.check_x52d)()) {
            tracing::info!("libx52's x52d running: {running}");
            notify(WatchEvent::X52dRunning(running));
        }
    }

    /// Applies a host `Disconnected` for `key`.
    fn disconnected(&mut self, key: L::Key, notify: &mut impl FnMut(WatchEvent)) {
        for change in self.selector.disconnected(&key) {
            tracing::info!("Device selection: {change:?}");
            notify(WatchEvent::Selection(change));
        }
        if !self.departed.contains(&key) {
            self.departed.push(key);
        }
    }

    /// Lists the devices and applies the list; a failed listing keeps the selection.
    fn relist(&mut self, notify: &mut impl FnMut(WatchEvent)) {
        let mut entries = match self.lister.list() {
            Ok(entries) => entries,
            Err(error) => {
                if !self.list_failed {
                    self.list_failed = true;
                    tracing::warn!("Cannot list the USB devices: {error}");
                }
                return;
            }
        };
        self.list_failed = false;
        self.departed
            .retain(|key| entries.iter().any(|entry| entry.key == *key));
        entries.retain(|entry| !self.departed.contains(&entry.key));
        for change in self.selector.update(&entries) {
            tracing::info!("Device selection: {change:?}");
            notify(WatchEvent::Selection(change));
        }
    }
}

#[cfg(test)]
mod tests;
