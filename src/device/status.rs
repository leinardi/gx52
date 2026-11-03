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

/// The device's state as the main thread sees it, built from [`DeviceEvent`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceStatus {
    /// Whether the device is there and can be opened.
    pub(crate) access: AccessState,
    /// The selected device's type.
    pub(crate) device_type: Option<DeviceType>,
    /// The active mode, while the evdev node is read.
    pub(crate) mode: Option<Mode>,
    /// Whether libx52's `x52d` is running.
    pub(crate) x52d_running: bool,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            access: AccessState::NoDevice,
            device_type: None,
            mode: None,
            x52d_running: false,
        }
    }
}

impl DeviceStatus {
    /// Applies `event`; true when the status changed.
    pub(crate) fn apply(&mut self, event: &DeviceEvent) -> bool {
        let before = *self;
        match event {
            DeviceEvent::Selected(device) => self.device_type = Some(device.device_type),
            DeviceEvent::Gone(_) => {
                self.device_type = None;
                self.mode = None;
            }
            DeviceEvent::Access(access) => self.access = *access,
            DeviceEvent::InputNode(node) => {
                if node.is_none() {
                    self.mode = None;
                }
            }
            DeviceEvent::Mode(mode) => self.mode = Some(*mode),
            DeviceEvent::X52dRunning(running) => self.x52d_running = *running,
            DeviceEvent::Error(_) | DeviceEvent::Sent(_) => {}
        }
        *self != before
    }

    /// The one line the window shows about the device.
    pub(crate) fn status_line(self) -> String {
        let Some(device_type) = self.device_type else {
            return gettext("No X52 joystick connected");
        };
        let device = device_name(device_type);
        let line = match (self.access, self.mode) {
            (AccessState::NoDevice, _) => gettext("{device} connected"),
            (AccessState::NoPermission, _) => {
                gettext("{device} connected, but GX52 is not allowed to control it")
            }
            (AccessState::Ready, None) => gettext("{device} ready"),
            (AccessState::Ready, Some(mode)) => {
                return gettext("{device} ready, mode {mode}")
                    .replace("{device}", device)
                    .replace("{mode}", mode_number(mode));
            }
        };
        line.replace("{device}", device)
    }

    /// Whether the window offers "Try again": only when the permission was denied.
    pub(crate) fn offers_retry(self) -> bool {
        self.device_type.is_some() && self.access == AccessState::NoPermission
    }

    /// The conflict warning, while `x52d` is running.
    pub(crate) fn x52d_warning(self) -> Option<String> {
        self.x52d_running.then(|| {
            gettext(
                "libx52’s x52d is running and will fight GX52 over the LEDs, the MFD and the clocks",
            )
        })
    }
}

/// The product name of `device_type`; product names are not translated.
const fn device_name(device_type: DeviceType) -> &'static str {
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

    #[test]
    fn no_device() {
        let status = DeviceStatus::default();
        assert_eq!(status.status_line(), "No X52 joystick connected");
        assert!(!status.offers_retry());
        assert_eq!(status.x52d_warning(), None);
    }

    #[test]
    fn found_but_not_opened_yet() {
        let status = status(AccessState::NoDevice, Some(DeviceType::X52Pro));
        assert_eq!(status.status_line(), "X52 Pro connected");
        assert!(!status.offers_retry());
    }

    #[test]
    fn no_permission_offers_try_again() {
        let status = status(AccessState::NoPermission, Some(DeviceType::X52));
        assert_eq!(
            status.status_line(),
            "X52 connected, but GX52 is not allowed to control it"
        );
        assert!(status.offers_retry());
    }

    #[test]
    fn ready_with_and_without_a_mode() {
        let mut status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        assert_eq!(status.status_line(), "X52 Pro ready");
        assert!(!status.offers_retry());
        status.mode = Some(Mode::Three);
        assert_eq!(status.status_line(), "X52 Pro ready, mode 3");
    }

    #[test]
    fn the_x52d_warning_shows_while_it_runs() {
        let mut status = status(AccessState::Ready, Some(DeviceType::X52Pro));
        status.x52d_running = true;
        assert_eq!(
            status.x52d_warning().as_deref(),
            Some(
                "libx52’s x52d is running and will fight GX52 over the LEDs, the MFD and the \
                 clocks"
            )
        );
        assert_eq!(
            status.status_line(),
            "X52 Pro ready",
            "the line is unchanged"
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
        assert!(!status.apply(&DeviceEvent::InputNode(Some(PathBuf::from(
            "/dev/input/event20"
        )))));
        assert!(status.apply(&DeviceEvent::Mode(Mode::Two)));
        assert!(status.apply(&DeviceEvent::X52dRunning(true)));
        assert!(!status.apply(&DeviceEvent::Error("busy".into())));
        assert_eq!(
            status,
            DeviceStatus {
                access: AccessState::Ready,
                device_type: Some(DeviceType::X52Pro),
                mode: Some(Mode::Two),
                x52d_running: true,
            }
        );

        assert!(status.apply(&DeviceEvent::InputNode(None)));
        assert_eq!(status.mode, None, "no mode without the evdev node");
        status.apply(&DeviceEvent::Mode(Mode::One));
        assert!(status.apply(&DeviceEvent::Gone(Generation(1))));
        assert_eq!(status.mode, None);
    }
}
