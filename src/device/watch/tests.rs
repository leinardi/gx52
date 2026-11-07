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

//! The selection over fake device lists, and the two sources over a scripted lister.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

use futures_lite::stream;

use super::*;

/// The keys of the fake lists: the attachment itself.
type Key = Attachment;

const PRO: (u16, u16) = (0x06a3, 0x0762);
const NON_PRO: (u16, u16) = (0x06a3, 0x0255);
const MOUSE: (u16, u16) = (0x046d, 0xc077);

fn entry(ids: (u16, u16), port: &str, address: u8) -> UsbEntry<Key> {
    let attachment = Attachment { bus: 1, address };
    UsbEntry {
        key: attachment,
        vendor_id: ids.0,
        product_id: ids.1,
        sysfs_path: PathBuf::from(format!("/sys/devices/pci0000:00/0000:00:14.0/usb1/{port}")),
        attachment,
    }
}

fn selected(device_type: DeviceType, port: &str, address: u8, generation: u64) -> SelectionChange {
    SelectionChange::Selected(DeviceId {
        device_type,
        sysfs_path: PathBuf::from(format!("/sys/devices/pci0000:00/0000:00:14.0/usb1/{port}")),
        attachment: Attachment { bus: 1, address },
        generation: Generation(generation),
    })
}

fn gone(generation: u64) -> SelectionChange {
    SelectionChange::Gone(Generation(generation))
}

#[test]
fn a_device_appears_and_disappears() {
    let mut selector = Selector::default();
    assert_eq!(selector.update(&[]), []);

    assert_eq!(
        selector.update(&[entry(PRO, "1-4", 5)]),
        [selected(DeviceType::X52Pro, "1-4", 5, 1)]
    );
    assert_eq!(selector.update(&[entry(PRO, "1-4", 5)]), [], "unchanged");

    assert_eq!(selector.update(&[]), [gone(1)]);
    assert_eq!(selector.selected(), None);
    assert_eq!(selector.update(&[]), []);
}

#[test]
fn the_first_device_in_sysfs_path_order_is_selected() {
    let mut selector = Selector::default();
    assert_eq!(
        selector.update(&[entry(NON_PRO, "1-6", 7), entry(PRO, "1-2", 5)]),
        [selected(DeviceType::X52Pro, "1-2", 5, 1)]
    );
}

#[test]
fn a_reordered_enumeration_keeps_the_selection() {
    let mut selector = Selector::default();
    selector.update(&[entry(PRO, "1-4", 5), entry(PRO, "1-6", 7)]);

    assert_eq!(
        selector.update(&[entry(PRO, "1-6", 7), entry(PRO, "1-4", 5)]),
        []
    );
    assert_eq!(selector.selected().unwrap().generation, Generation(1));
}

#[test]
fn a_second_x52_does_not_steal_the_selection() {
    let mut selector = Selector::default();
    selector.update(&[entry(PRO, "1-4", 5)]);

    // The newcomer sorts first, but the selection stays where it is.
    assert_eq!(
        selector.update(&[entry(NON_PRO, "1-1", 9), entry(PRO, "1-4", 5)]),
        []
    );

    // When the selected one goes, the other one is selected with a new generation.
    assert_eq!(
        selector.update(&[entry(NON_PRO, "1-1", 9)]),
        [gone(1), selected(DeviceType::X52, "1-1", 9, 2)]
    );
}

#[test]
fn the_same_model_on_another_port_is_a_new_identity_and_generation() {
    let mut selector = Selector::default();
    selector.update(&[entry(PRO, "1-4", 5)]);

    assert_eq!(
        selector.update(&[entry(PRO, "1-6", 6)]),
        [gone(1), selected(DeviceType::X52Pro, "1-6", 6, 2)]
    );
}

#[test]
fn a_new_attachment_at_the_same_path_is_a_new_generation() {
    // Same model, same port, the disconnect missed (between two polls, or faster than the
    // re-list): only the attachment differs.
    let mut selector = Selector::default();
    selector.update(&[entry(PRO, "1-4", 5)]);

    assert_eq!(
        selector.update(&[entry(PRO, "1-4", 6)]),
        [gone(1), selected(DeviceType::X52Pro, "1-4", 6, 2)]
    );
}

#[test]
fn a_host_disconnect_of_the_selected_attachment_is_gone() {
    let mut selector = Selector::default();
    selector.update(&[entry(PRO, "1-4", 5), entry(PRO, "1-6", 7)]);

    assert_eq!(
        selector.disconnected(&Attachment { bus: 1, address: 7 }),
        []
    );
    assert_eq!(
        selector.disconnected(&Attachment { bus: 1, address: 5 }),
        [gone(1)]
    );
    assert_eq!(selector.selected(), None);
    // The re-list after the event: unplugged, or plugged back in with a new attachment.
    assert_eq!(
        selector.update(&[entry(PRO, "1-4", 8)]),
        [selected(DeviceType::X52Pro, "1-4", 8, 2)]
    );
}

#[test]
fn a_non_x52_device_is_ignored() {
    let mut selector = Selector::default();
    assert_eq!(selector.update(&[entry(MOUSE, "1-1", 3)]), []);
    // Not even a Saitek vendor ID with an unknown product.
    assert_eq!(selector.update(&[entry((0x06a3, 0x1234), "1-2", 4)]), []);
    assert_eq!(selector.selected(), None);
}

#[test]
fn the_writer_message_carries_the_change() {
    let WriterMessage::Selected(device) = selected(DeviceType::X52Pro, "1-4", 5, 1).to_writer()
    else {
        panic!("not Selected");
    };
    assert_eq!(device.generation, Generation(1));
    assert!(matches!(
        gone(3).to_writer(),
        WriterMessage::Gone(Generation(3))
    ));
}

#[test]
fn the_reader_message_carries_the_change() {
    let change = selected(DeviceType::X52Pro, "1-4", 5, 1);
    let SelectionChange::Selected(device) = &change else {
        panic!("not Selected");
    };
    assert_eq!(change.to_reader(), ReaderMessage::Selected(device.clone()));
    assert_eq!(gone(3).to_reader(), ReaderMessage::Gone(Generation(3)));
}

/// The x52d check of a system without the daemon.
fn no_x52d() -> bool {
    false
}

/// The selection change of `event`, if it is one.
fn selection(event: WatchEvent) -> Option<SelectionChange> {
    match event {
        WatchEvent::Selection(change) => Some(change),
        WatchEvent::X52dRunning(_) => None,
    }
}

/// Returns the scripted lists in order, then the last one forever.
struct ScriptedLister {
    lists: VecDeque<io::Result<Vec<UsbEntry<Key>>>>,
    last: Vec<UsbEntry<Key>>,
}

impl ScriptedLister {
    fn new(lists: Vec<io::Result<Vec<UsbEntry<Key>>>>) -> Self {
        Self {
            lists: lists.into(),
            last: Vec::new(),
        }
    }
}

impl Lister for ScriptedLister {
    type Key = Key;

    fn list(&mut self) -> io::Result<Vec<UsbEntry<Key>>> {
        match self.lists.pop_front() {
            Some(Ok(list)) => {
                self.last.clone_from(&list);
                Ok(list)
            }
            Some(Err(error)) => Err(error),
            None => Ok(self.last.clone()),
        }
    }
}

#[test]
fn the_hotplug_source_relists_on_every_event_and_handles_a_disconnect() {
    let (trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![
        Ok(vec![entry(PRO, "1-4", 5)]),
        // After the disconnect, the device is not listed yet; then it is back.
        Ok(vec![]),
        Ok(vec![entry(PRO, "1-4", 6)]),
    ]);
    let events = stream::iter([
        Hotplug::Disconnected(Attachment { bus: 1, address: 5 }),
        Hotplug::Connected,
    ]);
    let (sender, changes) = mpsc::channel();
    let watch = thread::spawn(move || {
        run_hotplug(lister, events, no_x52d, &signal, |event| {
            if let Some(change) = selection(event) {
                sender.send(change).unwrap();
            }
        });
    });

    assert_eq!(
        changes.recv().unwrap(),
        selected(DeviceType::X52Pro, "1-4", 5, 1)
    );
    assert_eq!(changes.recv().unwrap(), gone(1));
    assert_eq!(
        changes.recv().unwrap(),
        selected(DeviceType::X52Pro, "1-4", 6, 2)
    );
    // The stream has ended; the watch returns at the end of the stream or at shutdown.
    drop(trigger);
    watch.join().unwrap();
}

#[test]
fn a_disconnected_attachment_still_listed_is_not_selected_again() {
    let (_trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![
        Ok(vec![entry(PRO, "1-4", 5)]),
        // The sysfs directory outlives the event for a moment.
        Ok(vec![entry(PRO, "1-4", 5)]),
        Ok(vec![entry(PRO, "1-4", 5)]),
        // Gone from the list, then plugged back in.
        Ok(vec![]),
        Ok(vec![entry(PRO, "1-4", 6)]),
    ]);
    let events = stream::iter([
        Hotplug::Disconnected(Attachment { bus: 1, address: 5 }),
        Hotplug::Connected,
        Hotplug::Connected,
        Hotplug::Connected,
    ]);
    let mut changes = Vec::new();

    run_hotplug(lister, events, no_x52d, &signal, |event| {
        changes.extend(selection(event));
    });

    assert_eq!(
        changes,
        [
            selected(DeviceType::X52Pro, "1-4", 5, 1),
            gone(1),
            selected(DeviceType::X52Pro, "1-4", 6, 2),
        ]
    );
}

#[test]
fn a_failed_listing_keeps_the_selection() {
    let (trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![
        Ok(vec![entry(PRO, "1-4", 5)]),
        Err(io::Error::other("sysfs went away")),
        Ok(vec![entry(PRO, "1-4", 5)]),
    ]);
    let events = stream::iter([Hotplug::Connected, Hotplug::Connected]);
    let mut changes = Vec::new();
    drop(trigger);

    run_hotplug(lister, events, no_x52d, &signal, |event| {
        changes.extend(selection(event));
    });

    assert_eq!(changes, [selected(DeviceType::X52Pro, "1-4", 5, 1)]);
}

/// Runs `watch` on a thread, signals shutdown and fails unless it returns within 500 ms.
fn assert_shutdown_within_500_ms(watch: impl FnOnce(&ShutdownSignal) + Send + 'static) {
    let (trigger, signal) = shutdown_signal();
    let (done, finished) = mpsc::channel();
    let thread = thread::spawn(move || {
        watch(&signal);
        done.send(()).unwrap();
    });
    // Let the watch reach its idle wait first.
    thread::sleep(Duration::from_millis(50));

    drop(trigger);

    finished
        .recv_timeout(Duration::from_millis(500))
        .expect("the watch did not stop within 500 ms");
    thread.join().unwrap();
}

#[test]
fn shutdown_of_an_idle_hotplug_stream_returns_within_500_ms() {
    assert_shutdown_within_500_ms(|signal| {
        let lister = ScriptedLister::new(vec![Ok(vec![entry(PRO, "1-4", 5)])]);
        run_hotplug(lister, stream::pending(), no_x52d, signal, |_| {});
    });
}

#[test]
fn shutdown_of_the_polling_loop_returns_within_500_ms() {
    assert_shutdown_within_500_ms(|signal| {
        let lister = ScriptedLister::new(vec![Ok(vec![entry(PRO, "1-4", 5)])]);
        run_polling(lister, no_x52d, signal, |_| {});
    });
}

#[test]
fn the_polling_source_follows_the_lists() {
    let (trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![
        Ok(vec![]),
        Ok(vec![entry(PRO, "1-4", 5)]),
        Ok(vec![entry(PRO, "1-4", 6)]),
    ]);
    let (sender, changes) = mpsc::channel();
    let watch = thread::spawn(move || {
        run_polling(lister, no_x52d, &signal, |event| {
            if let Some(change) = selection(event) {
                sender.send(change).unwrap();
            }
        });
    });

    assert_eq!(
        changes.recv().unwrap(),
        selected(DeviceType::X52Pro, "1-4", 5, 1)
    );
    assert_eq!(changes.recv().unwrap(), gone(1));
    assert_eq!(
        changes.recv().unwrap(),
        selected(DeviceType::X52Pro, "1-4", 6, 2)
    );
    drop(trigger);
    watch.join().unwrap();
}

#[test]
fn the_hotplug_source_relists_every_5_s_without_events() {
    let (trigger, signal) = shutdown_signal();
    // The disconnect's hotplug event was lost: the stream never yields.
    let lister = ScriptedLister::new(vec![Ok(vec![entry(PRO, "1-4", 5)]), Ok(vec![])]);
    let (sender, changes) = mpsc::channel();
    let watch = thread::spawn(move || {
        run_hotplug(lister, stream::pending(), no_x52d, &signal, |event| {
            if let Some(change) = selection(event) {
                sender.send(change).unwrap();
            }
        });
    });

    assert_eq!(
        changes.recv().unwrap(),
        selected(DeviceType::X52Pro, "1-4", 5, 1)
    );
    assert_eq!(
        changes.recv_timeout(Duration::from_secs(10)).unwrap(),
        gone(1)
    );
    drop(trigger);
    watch.join().unwrap();
}

#[test]
fn x52d_is_reported_at_start_then_on_change() {
    let (trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![Ok(vec![entry(PRO, "1-4", 5)])]);
    let mut events = Vec::new();
    drop(trigger);

    run_hotplug(
        lister,
        stream::pending(),
        || true,
        &signal,
        |event| {
            events.push(event);
        },
    );

    assert_eq!(
        events,
        [
            WatchEvent::X52dRunning(true),
            WatchEvent::Selection(selected(DeviceType::X52Pro, "1-4", 5, 1)),
        ]
    );
}

#[test]
fn the_polling_source_checks_x52d_every_5_s() {
    let (trigger, signal) = shutdown_signal();
    let lister = ScriptedLister::new(vec![Ok(Vec::new())]);
    let checks = Arc::new(Mutex::new(0_u32));
    let counted = Arc::clone(&checks);
    let (sender, events) = mpsc::channel();
    let watch = thread::spawn(move || {
        run_polling(
            lister,
            move || {
                let mut checks = counted.lock().unwrap();
                *checks += 1;
                // Running from the second check on.
                *checks > 1
            },
            &signal,
            |event| sender.send(event).unwrap(),
        );
    });

    assert_eq!(events.recv().unwrap(), WatchEvent::X52dRunning(false));
    // The second check comes 20 polls of 250 ms later.
    assert_eq!(
        events.recv_timeout(Duration::from_secs(10)).unwrap(),
        WatchEvent::X52dRunning(true)
    );
    drop(trigger);
    watch.join().unwrap();
    assert_eq!(*checks.lock().unwrap(), 2);
}
