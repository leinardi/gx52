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

//! What the app knows about the device, and how the window shows it.

use gettextrs::gettext;
use x52::device::DeviceType;
use x52::input::Mode;

use super::DeviceEvent;
use super::access::AccessState;
use crate::profile::ProfileKind;

/// The device's state as the main thread sees it, built from [`DeviceEvent`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceStatus {
    /// Whether the device is there and can be opened.
    pub(crate) access: AccessState,
    /// The selected device's type.
    pub(crate) device_type: Option<DeviceType>,
    /// The active mode, while the evdev node is read.
    pub(crate) mode: Option<Mode>,
    /// Whether the selected device's evdev node is open, so the input tester has input to show.
    pub(crate) input_open: bool,
    /// Whether libx52's `x52d` is running.
    pub(crate) x52d_running: bool,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            access: AccessState::NoDevice,
            device_type: None,
            mode: None,
            input_open: false,
            x52d_running: false,
        }
    }
}

impl DeviceStatus {
    /// Applies `event`; true when the status changed.
    pub(crate) fn apply(&mut self, event: &DeviceEvent) -> bool {
        let before = *self;
        match event {
            // A new selection starts without a node, and the earlier selection's `NodeClosed`
            // may never pass `CurrentSelection::accept`, so both clear `input_open` here.
            DeviceEvent::Selected(device) => {
                self.device_type = Some(device.device_type);
                self.input_open = false;
            }
            DeviceEvent::Gone(_) => {
                self.device_type = None;
                self.mode = None;
                self.input_open = false;
            }
            DeviceEvent::Access(access) => self.access = *access,
            DeviceEvent::InputNode { node, .. } => {
                self.input_open = node.is_some();
                if node.is_none() {
                    self.mode = None;
                }
            }
            DeviceEvent::Mode { mode, .. } => self.mode = Some(*mode),
            DeviceEvent::X52dRunning(running) => self.x52d_running = *running,
            DeviceEvent::Held { .. }
            | DeviceEvent::Axes { .. }
            | DeviceEvent::Error(_)
            | DeviceEvent::Sent(_)
            | DeviceEvent::Retried(_) => {}
        }
        *self != before
    }

    /// What the sidebar footer shows (docs/ui.md, "Device status"); `active_kind` is the kind of
    /// the active profile, since only a per-mode profile shows the mode.
    pub(crate) fn footer(self, active_kind: ProfileKind) -> Footer {
        let Some(device_type) = self.device_type else {
            return Footer {
                name: gettext("Not connected"),
                line: gettext("Waiting for an X52 or X52 Pro"),
                dot: Dot::Waiting,
                warning: false,
            };
        };
        let name = device_name(device_type).to_owned();
        if self.access == AccessState::NoPermission {
            return Footer {
                name,
                line: gettext("No permission"),
                dot: Dot::Problem,
                warning: false,
            };
        }
        if self.x52d_running {
            return Footer {
                name,
                line: gettext("x52d is running"),
                dot: Dot::Connected,
                warning: true,
            };
        }
        let line = match (active_kind, self.mode) {
            (ProfileKind::PerMode, Some(mode)) => {
                // Translators: {mode} is the position of the mode selector, 1, 2 or 3.
                gettext("Connected · Mode {mode}").replace("{mode}", mode_number(mode))
            }
            _ => gettext("Connected"),
        };
        Footer {
            name,
            line,
            dot: Dot::Connected,
            warning: false,
        }
    }

    /// The tray menu's status line (docs/ui.md, "Tray menu"): the device, the selector's mode for
    /// a per-mode profile and the active profile's name, or what keeps the device from using it.
    /// `selector` is the mode the application keeps (`Application::selector_mode`), not this
    /// status's, which a reopen of the evdev node clears and a new selection does not; `active`
    /// is the active profile's name and kind, once the profiles are loaded.
    pub(crate) fn tray_line(
        self,
        selector: Option<Mode>,
        active: Option<(&str, ProfileKind)>,
    ) -> String {
        let Some(device_type) = self.device_type else {
            return gettext("Not connected");
        };
        let device = device_name(device_type);
        let problem = |state: String| {
            // Translators: {device} is the joystick's name, {state} what keeps it from showing the
            // profile, as "No permission" or "x52d is running".
            gettext("{device} · {state}")
                .replace("{device}", device)
                .replace("{state}", &state)
        };
        if self.access == AccessState::NoPermission {
            return problem(gettext("No permission"));
        }
        if self.x52d_running {
            return problem(gettext("x52d is running"));
        }
        match (active, selector) {
            (Some((profile, ProfileKind::PerMode)), Some(mode)) => {
                // Translators: {device} is the joystick's name, {mode} the position of the mode
                // selector, 1, 2 or 3, and {profile} the active profile's name.
                gettext("{device} · Mode {mode} · {profile}")
                    .replace("{device}", device)
                    .replace("{mode}", mode_number(mode))
                    .replace("{profile}", profile)
            }
            (Some((profile, _)), _) => {
                // Translators: {device} is the joystick's name, {profile} the active profile's
                // name.
                gettext("{device} · {profile}")
                    .replace("{device}", device)
                    .replace("{profile}", profile)
            }
            (None, _) => problem(gettext("Connected")),
        }
    }

    /// The device type whose profiles the window shows: the selected device's, whether or not
    /// GX52 may open it, else the `last` one connected; `None` before any device was seen.
    pub(crate) fn shown_type(self, last: Option<DeviceType>) -> Option<DeviceType> {
        self.device_type.or(last)
    }

    /// Whether the device is there but GX52 may not open it: the window then shows how to give
    /// it access, with "Try again".
    pub(crate) fn offers_retry(self) -> bool {
        self.device_type.is_some() && self.access == AccessState::NoPermission
    }
}

/// The status dot of the footer; decorative, the text carries the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dot {
    /// Green: the device is there.
    Connected,
    /// Red: the device is there but cannot be used.
    Problem,
    /// An empty circle: no device.
    Waiting,
}

/// What the sidebar footer shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Footer {
    /// The device's name in bold, or "Not connected".
    pub(crate) name: String,
    /// The status line below it.
    pub(crate) line: String,
    /// The status dot.
    pub(crate) dot: Dot,
    /// Whether the line starts with a warning icon.
    pub(crate) warning: bool,
}

/// The product name of `device_type`; product names are not translated.
pub(crate) const fn device_name(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::X52Pro => "X52 Pro",
        DeviceType::X52 => "X52",
    }
}

/// The number of `mode` as the selector shows it.
const fn mode_number(mode: Mode) -> &'static str {
    match mode {
        Mode::One => "1",
        Mode::Two => "2",
        Mode::Three => "3",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::device::id::{Attachment, DeviceId, Generation};

    fn status(access: AccessState, device_type: Option<DeviceType>) -> DeviceStatus {
        DeviceStatus {
            access,
            device_type,
            ..DeviceStatus::default()
        }
    }

    fn footer(status: DeviceStatus, kind: ProfileKind) -> (String, String, Dot, bool) {
        let footer = status.footer(kind);
        (footer.name, footer.line, footer.dot, footer.warning)
    }

    fn texts(name: &str, line: &str, dot: Dot, warning: bool) -> (String, String, Dot, bool) {
        (name.to_owned(), line.to_owned(), dot, warning)
    }

    #[test]
    fn no_device() {
        let status = DeviceStatus::default();
        assert_eq!(
            footer(status, ProfileKind::PerMode),
            texts(
                "Not connected",
                "Waiting for an X52 or X52 Pro",
                Dot::Waiting,
                false
            )
        );
        assert!(!status.offers_retry());
    }

    #[test]
    fn found_but_not_opened_yet() {
        let status = status(AccessState::NoDevice, Some(DeviceType::X52Pro));
        assert_eq!(
            footer(status, ProfileKind::Single),
            texts("X52 Pro", "Connected", Dot::Connected, false)
        );
        assert!(!status.offers_retry());
    }

    #[test]
    fn no_permission_offers_try_again() {
        let mut status = status(AccessState::NoPermission, Some(DeviceType::X52));
        status.x52d_running = true;
        assert_eq!(
            footer(status, ProfileKind::Single),
            texts("X52", "No permission", Dot::Problem, false)
        );
        assert!(status.offers_retry());
    }

    #[test]
    fn a_device_without_permission_shows_its_own_type() {
        let denied_pro = status(AccessState::NoPermission, Some(DeviceType::X52Pro));
        assert_eq!(
            denied_pro.shown_type(None),
            Some(DeviceType::X52Pro),
            "no history"
        );
        let denied_x52 = status(AccessState::NoPermission, Some(DeviceType::X52));
        assert_eq!(
            denied_x52.shown_type(Some(DeviceType::X52Pro)),
            Some(DeviceType::X52),
            "another type connected before"
        );
    }

    #[test]
    fn without_a_device_the_last_type_is_shown() {
        let status = DeviceStatus::default();
        assert_eq!(
            status.shown_type(Some(DeviceType::X52)),
            Some(DeviceType::X52)
        );
        assert_eq!(status.shown_type(None), None);
    }

    #[test]
    fn only_a_per_mode_profile_shows_the_mode() {
        let mut status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        assert_eq!(
            footer(status, ProfileKind::PerMode),
            texts("X52 Pro", "Connected", Dot::Connected, false),
            "no mode reported yet"
        );
        status.mode = Some(Mode::Two);
        assert_eq!(
            footer(status, ProfileKind::PerMode),
            texts("X52 Pro", "Connected · Mode 2", Dot::Connected, false)
        );
        assert_eq!(
            footer(status, ProfileKind::Single),
            texts("X52 Pro", "Connected", Dot::Connected, false)
        );
        assert!(!status.offers_retry());
    }

    #[test]
    fn the_tray_line_names_the_device_mode_and_profile() {
        let status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        let two = Some(Mode::Two);
        assert_eq!(
            status.tray_line(two, Some(("DCS World", ProfileKind::PerMode))),
            "X52 Pro · Mode 2 · DCS World"
        );
        assert_eq!(
            status.tray_line(two, Some(("DCS World", ProfileKind::Single))),
            "X52 Pro · DCS World",
            "a single profile has no mode"
        );
        assert_eq!(
            status.tray_line(None, Some(("DCS World", ProfileKind::PerMode))),
            "X52 Pro · DCS World",
            "no mode reported yet"
        );
        assert_eq!(
            status.tray_line(two, None),
            "X52 Pro · Connected",
            "profiles not loaded yet"
        );
    }

    #[test]
    fn the_tray_line_shows_the_selector_not_the_status_mode() {
        let per_mode = Some(("DCS World", ProfileKind::PerMode));
        let mut status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        status.mode = Some(Mode::Two);
        // A reopen of the evdev node clears the status's mode; the application keeps the
        // selector's.
        let node = PathBuf::from("/dev/input/event7");
        status.apply(&DeviceEvent::InputNode {
            generation: Generation(1),
            node: None,
        });
        assert_eq!(status.mode, None);
        assert_eq!(
            status.tray_line(Some(Mode::Two), per_mode),
            "X52 Pro · Mode 2 · DCS World",
            "the mode survives the reopen"
        );
        // A new device keeps the status's mode of the old one until its reader reports; the
        // application has cleared the selector's.
        status.apply(&DeviceEvent::InputNode {
            generation: Generation(1),
            node: Some(node),
        });
        status.apply(&DeviceEvent::Mode {
            generation: Generation(1),
            mode: Mode::Three,
        });
        status.apply(&DeviceEvent::Selected(DeviceId {
            device_type: DeviceType::X52,
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-2"),
            attachment: Attachment { bus: 1, address: 9 },
            generation: Generation(2),
        }));
        assert_eq!(status.mode, Some(Mode::Three), "kept from the old device");
        assert_eq!(
            status.tray_line(None, per_mode),
            "X52 · DCS World",
            "no mode of the old device"
        );
    }

    #[test]
    fn the_tray_line_says_what_keeps_the_device_from_the_profile() {
        assert_eq!(
            DeviceStatus::default().tray_line(None, Some(("DCS World", ProfileKind::Single))),
            "Not connected"
        );
        let mut status = status(AccessState::NoPermission, Some(DeviceType::X52));
        status.x52d_running = true;
        assert_eq!(
            status.tray_line(None, Some(("DCS World", ProfileKind::Single))),
            "X52 · No permission"
        );
        status.access = AccessState::Ready;
        assert_eq!(
            status.tray_line(None, Some(("DCS World", ProfileKind::Single))),
            "X52 · x52d is running"
        );
    }

    #[test]
    fn x52d_shows_in_the_line() {
        let mut status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        status.x52d_running = true;
        status.mode = Some(Mode::Two);
        assert_eq!(
            footer(status, ProfileKind::PerMode),
            texts("X52 Pro", "x52d is running", Dot::Connected, true)
        );
    }

    #[test]
    fn events_update_the_status_and_report_changes() {
        let mut status = DeviceStatus::default();
        let device = DeviceId {
            device_type: DeviceType::X52Pro,
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-4"),
            attachment: Attachment { bus: 1, address: 5 },
            generation: Generation(1),
        };
        assert!(status.apply(&DeviceEvent::Selected(device)));
        assert!(status.apply(&DeviceEvent::Access(AccessState::Ready)));
        assert!(!status.apply(&DeviceEvent::Access(AccessState::Ready)));
        assert!(status.apply(&DeviceEvent::InputNode {
            generation: Generation(1),
            node: Some(PathBuf::from("/dev/input/event20")),
        }));
        assert!(status.apply(&DeviceEvent::Mode {
            generation: Generation(1),
            mode: Mode::Two,
        }));
        assert!(status.apply(&DeviceEvent::X52dRunning(true)));
        assert!(!status.apply(&DeviceEvent::Error("busy".into())));
        assert_eq!(
            status,
            DeviceStatus {
                access: AccessState::Ready,
                device_type: Some(DeviceType::X52Pro),
                mode: Some(Mode::Two),
                input_open: true,
                x52d_running: true,
            }
        );

        assert!(status.apply(&DeviceEvent::InputNode {
            generation: Generation(1),
            node: None,
        }));
        assert_eq!(status.mode, None, "no mode without the evdev node");
        status.apply(&DeviceEvent::Mode {
            generation: Generation(1),
            mode: Mode::One,
        });
        assert!(status.apply(&DeviceEvent::Gone(Generation(1))));
        assert_eq!(status.mode, None);
    }

    fn selected(generation: u64) -> DeviceEvent {
        DeviceEvent::Selected(DeviceId {
            device_type: DeviceType::X52Pro,
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-4"),
            attachment: Attachment { bus: 1, address: 5 },
            generation: Generation(generation),
        })
    }

    fn node(generation: u64, open: bool) -> DeviceEvent {
        DeviceEvent::InputNode {
            generation: Generation(generation),
            node: open.then(|| PathBuf::from("/dev/input/event20")),
        }
    }

    /// The events as the application applies them: only those `CurrentSelection` accepts.
    fn input_open_after(events: &[DeviceEvent]) -> bool {
        let mut selection = crate::device::CurrentSelection::default();
        let mut status = DeviceStatus::default();
        for event in events {
            if selection.accept(event) {
                status.apply(event);
            }
        }
        status.input_open
    }

    #[test]
    fn the_node_is_open_only_between_its_open_and_close() {
        assert!(input_open_after(&[selected(1), node(1, true)]));
        assert!(!input_open_after(&[selected(1)]), "not opened yet");
        assert!(!input_open_after(&[
            selected(1),
            node(1, true),
            node(1, false)
        ]));
        assert!(
            input_open_after(&[selected(1), node(1, true), node(1, false), node(1, true)]),
            "reopened"
        );
    }

    #[test]
    fn a_dropped_close_of_an_earlier_selection_leaves_no_node_open() {
        // The `NodeClosed` of generation 1 arrives after its `Gone` and is dropped.
        assert!(!input_open_after(&[
            selected(1),
            node(1, true),
            DeviceEvent::Gone(Generation(1)),
            node(1, false),
        ]));
        // A replacement whose node never opens: generation 1's close is dropped after it.
        assert!(!input_open_after(&[
            selected(1),
            node(1, true),
            selected(2),
            node(1, false),
        ]));
    }
}
