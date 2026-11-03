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

//! The identity of the selected device.

use std::fmt;
use std::path::PathBuf;

use x52::device::DeviceType;

/// The selected device: which physical device, which attachment of it and which selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DeviceId {
    /// The type, from the vendor and product ID.
    pub(crate) device_type: DeviceType,
    /// The USB device's sysfs directory: the selection key. It chooses the physical device and
    /// survives a replug into the same port.
    pub(crate) sysfs_path: PathBuf,
    /// This attachment of the device. It changes on every replug and is never used to choose a
    /// device.
    pub(crate) attachment: Attachment,
    /// Incremented by the device watch whenever the selection key or the attachment changes, so
    /// a replug into the same port is a new generation: a reopen and a full replay.
    pub(crate) generation: Generation,
}

/// One attachment of a USB device: what `nusb::DeviceInfo::id()` identifies on Linux, the bus
/// number and the device address, which the kernel assigns anew on every attachment.
///
/// `nusb` keeps its `DeviceId` opaque; this is the same value in a form the tests can build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Attachment {
    /// The bus number.
    pub(crate) bus: u8,
    /// The device address on that bus.
    pub(crate) address: u8,
}

impl Attachment {
    /// The attachment `info` describes.
    pub(crate) fn of(info: &nusb::DeviceInfo) -> Self {
        Self {
            bus: info.busnum(),
            address: info.device_address(),
        }
    }
}

/// A selection's sequence number: each new selection, or new attachment of the selected device,
/// gets the next one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Generation(pub(crate) u64);

impl fmt::Display for Generation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}
