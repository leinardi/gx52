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

//! The USB side of the device: opening the selected device and sending control transfers to it
//! with `nusb` (docs/protocol.md, "USB vendor commands").

use std::io;

use nusb::MaybeFuture;
use nusb::transfer::{ControlOut, ControlType, Recipient, TransferError};
use x52::command::ControlTransfer;
use x52::transport::{self, Transport};

use super::id::{Attachment, DeviceId};

/// An opened X52 that sends vendor control transfers.
///
/// `ControlType::Vendor`, `Recipient::Device` and the host-to-device direction of `control_out`
/// make up [`transport::REQUEST_TYPE`] (`0x40`).
#[derive(Debug)]
pub(crate) struct NusbTransport {
    device: nusb::Device,
}

impl Transport for NusbTransport {
    type Error = TransferError;

    fn control_out(&mut self, transfer: ControlTransfer) -> Result<(), Self::Error> {
        self.device
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: transport::REQUEST,
                    value: transfer.value(),
                    index: transfer.index(),
                    data: &[],
                },
                transport::TIMEOUT,
            )
            .wait()
    }
}

/// Why the selected device could not be opened.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OpenError {
    /// `EACCES`: the device is listed, but the user may not open it (no udev rule).
    #[error("permission denied")]
    PermissionDenied,
    /// The selected attachment is no longer listed.
    #[error("the device is no longer listed")]
    NotListed,
    /// Any other failure.
    #[error(transparent)]
    Failed(io::Error),
}

/// Opens the selected device; the device writer is generic over it so the tests can use a fake.
pub(crate) trait Opener {
    /// What an opened device sends transfers through.
    type Transport: Transport;

    /// Opens `device`: the attachment it names and no other.
    ///
    /// # Errors
    ///
    /// [`OpenError::PermissionDenied`] on `EACCES`, told apart from every other failure.
    fn open(&mut self, device: &DeviceId) -> Result<Self::Transport, OpenError>;
}

/// The [`Opener`] of the real device, through `nusb`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NusbOpener;

impl Opener for NusbOpener {
    type Transport = NusbTransport;

    fn open(&mut self, device: &DeviceId) -> Result<NusbTransport, OpenError> {
        let info = nusb::list_devices()
            .wait()
            .map_err(|error| OpenError::Failed(error.into()))?
            .find(|info| {
                info.sysfs_path() == device.sysfs_path && Attachment::of(info) == device.attachment
            })
            .ok_or(OpenError::NotListed)?;
        info.open()
            .wait()
            .map(|device| NusbTransport { device })
            .map_err(|error| {
                if error.kind() == nusb::ErrorKind::PermissionDenied {
                    OpenError::PermissionDenied
                } else {
                    OpenError::Failed(error.into())
                }
            })
    }
}
