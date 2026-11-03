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

//! The typed commands of docs/protocol.md ("Commands the app may send") and their encoding.
//!
//! Every value a command carries is built through a checked constructor, and a
//! [`ControlTransfer`] can only be built by [`Command::encode`], so nothing outside the allowlist
//! can reach a transport.

use crate::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};
use crate::error::RangeError;

/// The `wIndex` of every command, as listed in docs/protocol.md.
mod index {
    pub(super) const MFD_BRIGHTNESS: u16 = 0xb1;
    pub(super) const LED_BRIGHTNESS: u16 = 0xb2;
    pub(super) const BLINK: u16 = 0xb4;
    pub(super) const LED: u16 = 0xb8;
    pub(super) const CLOCK_1: u16 = 0xc0;
    pub(super) const CLOCK_2: u16 = 0xc1;
    pub(super) const CLOCK_3: u16 = 0xc2;
    pub(super) const DATE_FIRST_SECOND: u16 = 0xc4;
    pub(super) const DATE_THIRD: u16 = 0xc8;
    pub(super) const MFD_LINE_1: u16 = 0xd1;
    pub(super) const MFD_LINE_2: u16 = 0xd2;
    pub(super) const MFD_LINE_3: u16 = 0xd4;
    /// Added to a line's write command to clear the line.
    pub(super) const MFD_CLEAR: u16 = 0x08;
    pub(super) const SHIFT: u16 = 0xfd;
}

/// The `wValue` of shift and blink.
const ON: u16 = 0x51;
const OFF: u16 = 0x50;

/// The byte an odd-length MFD line is padded with: the space.
const PAD: u8 = 0x20;

/// One encoded vendor control transfer: a command (`wIndex`) and its argument (`wValue`).
///
/// Built only by [`Command::encode`]; the request type, request and timeout are the constants of
/// [`crate::transport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlTransfer {
    index: u16,
    value: u16,
}

impl ControlTransfer {
    const fn new(index: u16, value: u16) -> Self {
        Self { index, value }
    }

    /// The command, sent as `wIndex`.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }

    /// The argument, sent as `wValue`.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.value
    }
}

/// A brightness level on the wire scale, `0`–`128`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Brightness(u8);

impl Brightness {
    /// The highest level the app sends (`255` looks the same on the device and is never sent).
    pub const MAX: u8 = 128;

    /// A brightness level.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] above `128`.
    pub fn new(level: u8) -> Result<Self, RangeError> {
        if level > Self::MAX {
            return Err(RangeError::out_of_range("brightness", level, 0, Self::MAX));
        }
        Ok(Self(level))
    }

    /// The level.
    #[must_use]
    pub const fn level(self) -> u8 {
        self.0
    }
}

/// The time of day clock 1 shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClockTime {
    hour: u8,
    minute: u8,
}

impl ClockTime {
    /// A time of day, `hour` `0`–`23` and `minute` `0`–`59`.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] for an hour or a minute outside its range.
    pub fn new(hour: u8, minute: u8) -> Result<Self, RangeError> {
        if hour > 23 {
            return Err(RangeError::out_of_range("hour", hour, 0, 23));
        }
        if minute > 59 {
            return Err(RangeError::out_of_range("minute", minute, 0, 59));
        }
        Ok(Self { hour, minute })
    }

    /// The hour, `0`–`23`.
    #[must_use]
    pub const fn hour(self) -> u8 {
        self.hour
    }

    /// The minute, `0`–`59`.
    #[must_use]
    pub const fn minute(self) -> u8 {
        self.minute
    }
}

/// The offset of clock 2 or 3 from clock 1, in minutes, normalised into `-720`–`+720`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClockOffset(i16);

impl ClockOffset {
    /// The largest magnitude: half a day, which every normalised offset fits.
    pub const MAX_MINUTES: i16 = 720;

    /// An offset in minutes, `-720`–`+720`.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] outside `-720`–`+720`.
    pub fn new(minutes: i16) -> Result<Self, RangeError> {
        if !(-Self::MAX_MINUTES..=Self::MAX_MINUTES).contains(&minutes) {
            return Err(RangeError::out_of_range(
                "clock offset",
                minutes,
                -Self::MAX_MINUTES,
                Self::MAX_MINUTES,
            ));
        }
        Ok(Self(minutes))
    }

    /// The offset in minutes.
    #[must_use]
    pub const fn minutes(self) -> i16 {
        self.0
    }
}

/// The three two-digit fields of the date, in the order the MFD shows them
/// (`first.second.third`). The date format decides which of day, month and year goes where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DateFields {
    first: u8,
    second: u8,
    third: u8,
}

impl DateFields {
    /// The largest value of a field.
    pub const MAX: u8 = 99;

    /// Three fields, each `0`–`99`.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] for a field above `99`.
    pub fn new(first: u8, second: u8, third: u8) -> Result<Self, RangeError> {
        for (field, value) in [
            ("first date field", first),
            ("second date field", second),
            ("third date field", third),
        ] {
            if value > Self::MAX {
                return Err(RangeError::out_of_range(field, value, 0, Self::MAX));
            }
        }
        Ok(Self {
            first,
            second,
            third,
        })
    }

    /// The first field shown.
    #[must_use]
    pub const fn first(self) -> u8 {
        self.first
    }

    /// The second field shown.
    #[must_use]
    pub const fn second(self) -> u8 {
        self.second
    }

    /// The third field shown.
    #[must_use]
    pub const fn third(self) -> u8 {
        self.third
    }
}

/// A line of the MFD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MfdLine {
    /// The top line.
    One,
    /// The middle line.
    Two,
    /// The bottom line.
    Three,
}

impl MfdLine {
    /// Every line, top to bottom.
    pub const ALL: [Self; 3] = [Self::One, Self::Two, Self::Three];

    const fn write_index(self) -> u16 {
        match self {
            Self::One => index::MFD_LINE_1,
            Self::Two => index::MFD_LINE_2,
            Self::Three => index::MFD_LINE_3,
        }
    }
}

/// The bytes of one MFD line in the device's character set: at most 16, none of them blank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MfdBytes {
    bytes: [u8; Self::MAX_LEN],
    len: usize,
}

impl MfdBytes {
    /// The bytes a line holds.
    pub const MAX_LEN: usize = 16;

    /// A line of character-set bytes.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] for more than 16 bytes, [`RangeError::BlankByte`] for a byte the
    /// MFD shows as blank (`0x00`–`0x07`, `0xA0`).
    pub fn new(bytes: &[u8]) -> Result<Self, RangeError> {
        let len = bytes.len();
        if len > Self::MAX_LEN {
            return Err(RangeError::out_of_range(
                "MFD line length",
                i64::try_from(len).unwrap_or(i64::MAX),
                0,
                16,
            ));
        }
        if let Some((position, byte)) = bytes.iter().enumerate().find(|(_, byte)| is_blank(**byte))
        {
            return Err(RangeError::BlankByte {
                byte: *byte,
                position,
            });
        }
        let mut line = [0; Self::MAX_LEN];
        line[..len].copy_from_slice(bytes);
        Ok(Self { bytes: line, len })
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Whether the MFD shows `byte` as blank (docs/protocol.md, "MFD character set").
const fn is_blank(byte: u8) -> bool {
    matches!(byte, 0x00..=0x07 | 0xa0)
}

/// Clock 2 or clock 3, the clocks set as an offset from clock 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OffsetClock {
    /// Clock 2.
    Clock2,
    /// Clock 3.
    Clock3,
}

/// A command the app may send: the allowlist of docs/protocol.md as a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    /// The MFD backlight (`0xb1`).
    MfdBrightness(Brightness),
    /// Every LED, POV 1 included (`0xb2`).
    LedBrightness(Brightness),
    /// The i and POV 1 LEDs blink (`0xb4`); X52 Pro only.
    Blink(bool),
    /// A bi-colour LED (`0xb8`, green bit then red bit); X52 Pro only.
    Led(BiColourLed, LedColour),
    /// A single-colour LED on or off (`0xb8`); X52 Pro only.
    LedSwitch(SingleColourLed, bool),
    /// The time clock 1 shows (`0xc0`), in 24 h or 12 h format.
    Clock1 {
        /// The time of day.
        time: ClockTime,
        /// 24 h when true, 12 h with an AM/PM indicator otherwise.
        h24: bool,
    },
    /// The offset of clock 2 (`0xc1`) or clock 3 (`0xc2`) from clock 1.
    ClockOffset {
        /// Which clock.
        clock: OffsetClock,
        /// The normalised offset.
        offset: ClockOffset,
        /// 24 h when true, 12 h otherwise.
        h24: bool,
    },
    /// The date (`0xc4`, then `0xc8`).
    Date(DateFields),
    /// The text of an MFD line: a clear (`line | 0x08`), then the bytes two per write.
    MfdText(MfdLine, MfdBytes),
    /// The SHIFT label on the MFD (`0xfd`); X52 Pro only.
    Shift(bool),
}

impl Command {
    /// A short lowercase name, for messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::MfdBrightness(_) => "MFD brightness",
            Self::LedBrightness(_) => "LED brightness",
            Self::Blink(_) => "blink",
            Self::Led(..) | Self::LedSwitch(..) => "LED",
            Self::Clock1 { .. } => "clock 1",
            Self::ClockOffset { .. } => "clock offset",
            Self::Date(_) => "date",
            Self::MfdText(..) => "MFD text",
            Self::Shift(_) => "shift",
        }
    }

    /// Whether `device` has this command: LEDs, blink and shift are X52 Pro only.
    #[must_use]
    pub const fn supported_by(&self, device: DeviceType) -> bool {
        match self {
            Self::Led(..) | Self::LedSwitch(..) => device.has_leds(),
            Self::Blink(_) => device.has_blink(),
            Self::Shift(_) => device.has_shift(),
            Self::MfdBrightness(_)
            | Self::LedBrightness(_)
            | Self::Clock1 { .. }
            | Self::ClockOffset { .. }
            | Self::Date(_)
            | Self::MfdText(..) => true,
        }
    }

    /// The control transfers that carry this command, in the order they are sent.
    #[must_use]
    pub fn encode(&self) -> Vec<ControlTransfer> {
        match *self {
            Self::MfdBrightness(level) => {
                vec![ControlTransfer::new(
                    index::MFD_BRIGHTNESS,
                    u16::from(level.level()),
                )]
            }
            Self::LedBrightness(level) => {
                vec![ControlTransfer::new(
                    index::LED_BRIGHTNESS,
                    u16::from(level.level()),
                )]
            }
            Self::Blink(on) => vec![ControlTransfer::new(index::BLINK, switch(on))],
            Self::Led(led, colour) => vec![
                led_bit(led.green_bit(), colour.green()),
                led_bit(led.red_bit(), colour.red()),
            ],
            Self::LedSwitch(led, on) => vec![led_bit(led.bit(), on)],
            Self::Clock1 { time, h24 } => vec![ControlTransfer::new(
                index::CLOCK_1,
                u16::from(h24) << 15 | u16::from(time.hour()) << 8 | u16::from(time.minute()),
            )],
            Self::ClockOffset { clock, offset, h24 } => {
                let command = match clock {
                    OffsetClock::Clock2 => index::CLOCK_2,
                    OffsetClock::Clock3 => index::CLOCK_3,
                };
                let minutes = offset.minutes();
                vec![ControlTransfer::new(
                    command,
                    u16::from(h24) << 15 | u16::from(minutes < 0) << 10 | minutes.unsigned_abs(),
                )]
            }
            Self::Date(fields) => vec![
                ControlTransfer::new(
                    index::DATE_FIRST_SECOND,
                    pair(fields.first(), fields.second()),
                ),
                ControlTransfer::new(index::DATE_THIRD, u16::from(fields.third())),
            ],
            Self::MfdText(line, bytes) => {
                let write = line.write_index();
                let mut transfers = vec![ControlTransfer::new(write | index::MFD_CLEAR, 0)];
                transfers.extend(bytes.as_bytes().chunks(2).map(|chunk| {
                    let first = chunk.first().copied().unwrap_or(PAD);
                    let second = chunk.get(1).copied().unwrap_or(PAD);
                    ControlTransfer::new(write, pair(first, second))
                }));
                transfers
            }
            Self::Shift(on) => vec![ControlTransfer::new(index::SHIFT, switch(on))],
        }
    }
}

/// The `wValue` of shift and blink.
const fn switch(on: bool) -> u16 {
    if on { ON } else { OFF }
}

/// One `0xb8` transfer: `bit << 8 | state`.
fn led_bit(bit: u8, on: bool) -> ControlTransfer {
    ControlTransfer::new(index::LED, u16::from(bit) << 8 | u16::from(on))
}

/// Two bytes in one `wValue`, the first in the low byte.
fn pair(first: u8, second: u8) -> u16 {
    u16::from(second) << 8 | u16::from(first)
}

#[cfg(test)]
mod tests;
