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

use super::*;
use crate::command::{Brightness, MfdBytes, MfdLine};
use crate::device::{BiColourLed, LedColour, SingleColourLed};

#[test]
fn pro_only_commands_are_refused_on_the_x52_before_any_transfer() {
    for command in [
        Command::Led(BiColourLed::ButtonA, LedColour::Green),
        Command::LedSwitch(SingleColourLed::Fire, true),
        Command::Shift(true),
        Command::Blink(true),
    ] {
        let mut transport = RecordingTransport::new();
        let result = send(&mut transport, DeviceType::X52, &command);
        assert!(
            matches!(
                result,
                Err(SendError::Unsupported {
                    device: DeviceType::X52,
                    ..
                })
            ),
            "{command:?}: {result:?}"
        );
        assert!(transport.transfers().is_empty(), "{command:?}");
    }
}

#[test]
fn shared_commands_are_sent_to_the_x52() {
    let mut transport = RecordingTransport::new();
    let command =
        Command::MfdBrightness(Brightness::new(64).unwrap_or_else(|error| panic!("{error}")));
    assert!(send(&mut transport, DeviceType::X52, &command).is_ok());
    assert_eq!(transport.pairs(), [(0xb1, 64)]);
}

#[test]
fn unsupported_message_names_the_device_and_the_command() {
    let mut transport = RecordingTransport::new();
    let error = send(&mut transport, DeviceType::X52, &Command::Shift(true))
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        error.as_deref(),
        Some("the x52 does not support the shift command")
    );
}

#[test]
fn a_failed_transfer_stops_the_command() {
    let bytes = MfdBytes::new(b"abcdef").unwrap_or_else(|error| panic!("{error}"));
    let mut transport = RecordingTransport::failing_at(2);
    let result = send(
        &mut transport,
        DeviceType::X52Pro,
        &Command::MfdText(MfdLine::One, bytes),
    );
    let Err(SendError::Transport(failure)) = result else {
        panic!("expected a transport error, got {result:?}");
    };
    assert_eq!(failure.call(), 2);
    // The clear and the first write went out; the write that failed and the one after did not.
    assert_eq!(transport.pairs(), [(0xd9, 0), (0xd1, 0x6261)]);
}

#[test]
fn a_recording_transport_records_after_the_injected_failure() {
    let mut transport = RecordingTransport::failing_at(0);
    let command = Command::Shift(true);
    assert!(send(&mut transport, DeviceType::X52Pro, &command).is_err());
    assert!(send(&mut transport, DeviceType::X52Pro, &command).is_ok());
    assert_eq!(transport.pairs(), [(0xfd, 0x51)]);
}
