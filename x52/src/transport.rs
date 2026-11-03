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

//! The transport that carries encoded commands to a device, and the one way to send a command.
//!
//! The app implements [`Transport`] with `nusb` (docs/protocol.md, "USB vendor commands"); the
//! tests use the [`RecordingTransport`] (feature `test-support`).

use std::time::Duration;

use crate::command::{Command, ControlTransfer};
use crate::device::DeviceType;

/// `bmRequestType`: host to device, vendor, recipient device.
pub const REQUEST_TYPE: u8 = 0x40;

/// `bRequest` of every command.
pub const REQUEST: u8 = 0x91;

/// The timeout of one control transfer (GX52 0.7).
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Something that sends one encoded control transfer to a device.
///
/// Implementations send the transfer as a vendor control transfer with [`REQUEST_TYPE`],
/// [`REQUEST`] and [`TIMEOUT`], with no data stage.
pub trait Transport {
    /// Why a transfer failed.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Sends one control transfer.
    ///
    /// # Errors
    ///
    /// The transport's error when the transfer fails.
    fn control_out(&mut self, transfer: ControlTransfer) -> Result<(), Self::Error>;
}

/// Why a command was not sent, or not sent completely.
#[derive(Debug, thiserror::Error)]
pub enum SendError<E> {
    /// The device type does not have the command; nothing was sent.
    #[error("the {device} does not support the {name} command")]
    Unsupported {
        /// The device type.
        device: DeviceType,
        /// The command's name.
        name: &'static str,
    },
    /// A transfer failed; the transfers before it were sent, the ones after it were not.
    #[error("a control transfer failed")]
    Transport(#[source] E),
}

/// Sends `command` to a device of type `device`, transfer by transfer, stopping at the first
/// failure.
///
/// # Errors
///
/// [`SendError::Unsupported`], before any transfer, when `device` does not have the command;
/// [`SendError::Transport`] when a transfer fails.
pub fn send<T: Transport + ?Sized>(
    transport: &mut T,
    device: DeviceType,
    command: &Command,
) -> Result<(), SendError<T::Error>> {
    if !command.supported_by(device) {
        return Err(SendError::Unsupported {
            device,
            name: command.name(),
        });
    }
    for transfer in command.encode() {
        transport
            .control_out(transfer)
            .map_err(SendError::Transport)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
pub use recording::{InjectedFailure, RecordingTransport};

#[cfg(any(test, feature = "test-support"))]
mod recording {
    use super::Transport;
    use crate::command::ControlTransfer;

    /// A transport that records every transfer instead of sending it, for tests.
    ///
    /// It can fail one call, to test what happens after a failed transfer.
    #[derive(Debug, Default)]
    pub struct RecordingTransport {
        transfers: Vec<ControlTransfer>,
        calls: usize,
        fail_at: Option<usize>,
    }

    impl RecordingTransport {
        /// A transport that records every transfer.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// A transport whose call number `call` (counted from 0) fails and is not recorded; the
        /// calls before and after it are recorded.
        #[must_use]
        pub fn failing_at(call: usize) -> Self {
            Self {
                fail_at: Some(call),
                ..Self::default()
            }
        }

        /// The transfers recorded so far, in the order they were sent.
        #[must_use]
        pub fn transfers(&self) -> &[ControlTransfer] {
            &self.transfers
        }

        /// The recorded transfers as `(wIndex, wValue)` pairs.
        #[must_use]
        pub fn pairs(&self) -> Vec<(u16, u16)> {
            self.transfers
                .iter()
                .map(|transfer| (transfer.index(), transfer.value()))
                .collect()
        }
    }

    impl Transport for RecordingTransport {
        type Error = InjectedFailure;

        fn control_out(&mut self, transfer: ControlTransfer) -> Result<(), Self::Error> {
            let call = self.calls;
            self.calls += 1;
            if self.fail_at == Some(call) {
                return Err(InjectedFailure { call });
            }
            self.transfers.push(transfer);
            Ok(())
        }
    }

    /// The failure a [`RecordingTransport`] was told to inject.
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    #[error("injected failure at call {call}")]
    pub struct InjectedFailure {
        call: usize,
    }

    impl InjectedFailure {
        /// The call that failed, counted from 0.
        #[must_use]
        pub const fn call(&self) -> usize {
            self.call
        }
    }
}

#[cfg(test)]
mod tests;
