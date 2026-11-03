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

//! What the app can do with the selected device (docs/protocol.md, "Access states").

use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// Whether a device is there and whether it can be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AccessState {
    /// No supported device is selected.
    NoDevice,
    /// A supported device is listed, but opening it fails with `EACCES`: input works, the LEDs,
    /// the MFD and the clocks do not.
    NoPermission,
    /// The device is open.
    Ready,
}

/// Whether the virtual mouse can create its uinput device, checked apart from the USB access and
/// only on request: a missing uinput permission does not affect the LEDs, the MFD or the clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UinputAccess {
    /// `/dev/uinput` opens for writing.
    Available,
    /// Opening `/dev/uinput` for writing fails with `EACCES` (no udev rule for it).
    NoPermission,
    /// There is no `/dev/uinput` (the module is not loaded, or the sandbox does not show it).
    Missing,
    /// Opening fails for another reason.
    Failed(io::ErrorKind),
}

/// The uinput device node.
const UINPUT: &str = "/dev/uinput";

/// Checks the uinput access by opening `/dev/uinput` for writing, then closing it.
pub(crate) fn check_uinput() -> UinputAccess {
    check_uinput_at(Path::new(UINPUT))
}

/// Checks the uinput access of the node at `path`.
fn check_uinput_at(path: &Path) -> UinputAccess {
    match OpenOptions::new().write(true).open(path) {
        Ok(_) => UinputAccess::Available,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => UinputAccess::NoPermission,
        Err(error) if error.kind() == io::ErrorKind::NotFound => UinputAccess::Missing,
        Err(error) => UinputAccess::Failed(error.kind()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn a_writable_node_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let node = dir.path().join("uinput");
        fs::write(&node, "").unwrap();
        assert_eq!(check_uinput_at(&node), UinputAccess::Available);
    }

    #[test]
    fn a_read_only_node_is_no_permission() {
        let dir = tempfile::tempdir().unwrap();
        let node = dir.path().join("uinput");
        fs::write(&node, "").unwrap();
        fs::set_permissions(&node, Permissions::from_mode(0o400)).unwrap();
        // Root opens it anyway; the check is only meaningful for a normal user.
        if OpenOptions::new().write(true).open(&node).is_err() {
            assert_eq!(check_uinput_at(&node), UinputAccess::NoPermission);
        }
    }

    #[test]
    fn no_node_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            check_uinput_at(&dir.path().join("uinput")),
            UinputAccess::Missing
        );
    }
}
