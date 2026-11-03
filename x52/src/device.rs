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

//! The supported devices, what each one can do, and the LEDs of the X52 Pro.
//!
//! The IDs, capabilities and LED bits are those of docs/protocol.md ("Device IDs and access", "LED bits",
//! "X52 (non-Pro)"). A device whose vendor and product ID are not listed here is never driven.

use std::fmt;

/// The USB vendor ID of every supported device (Saitek, now Logitech).
pub const VENDOR_ID: u16 = 0x06a3;

/// The product IDs and the device type each one is, in the order of docs/protocol.md.
const PRODUCTS: [(u16, DeviceType); 3] = [
    (0x0762, DeviceType::X52Pro),
    (0x0255, DeviceType::X52),
    (0x075c, DeviceType::X52),
];

/// A supported device type.
///
/// The type decides which commands exist: the X52 has no LED control, shift indicator or blink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceType {
    /// The X52 Pro Flight Control System.
    X52Pro,
    /// The X52 Flight Controller (non-Pro), supported on a best-effort basis.
    X52,
}

impl DeviceType {
    /// Every device type.
    pub const ALL: [Self; 2] = [Self::X52Pro, Self::X52];

    /// The device type of a USB vendor and product ID, or `None` for any device that is not
    /// listed in docs/protocol.md.
    #[must_use]
    pub fn from_ids(vendor: u16, product: u16) -> Option<Self> {
        if vendor != VENDOR_ID {
            return None;
        }
        PRODUCTS
            .iter()
            .find(|(known, _)| *known == product)
            .map(|(_, device)| *device)
    }

    /// The stable identifier used in profiles: `x52-pro` or `x52`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::X52Pro => "x52-pro",
            Self::X52 => "x52",
        }
    }

    /// Whether the LEDs can be set (`0xb8`): X52 Pro only.
    #[must_use]
    pub const fn has_leds(self) -> bool {
        matches!(self, Self::X52Pro)
    }

    /// Whether the SHIFT label on the MFD can be lit (`0xfd`): X52 Pro only, until verified on an
    /// X52.
    #[must_use]
    pub const fn has_shift(self) -> bool {
        matches!(self, Self::X52Pro)
    }

    /// Whether the i and POV 1 LEDs can blink (`0xb4`): X52 Pro only, until verified on an X52.
    #[must_use]
    pub const fn has_blink(self) -> bool {
        matches!(self, Self::X52Pro)
    }
}

impl fmt::Display for DeviceType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

/// A red and green LED of the X52 Pro.
///
/// The POV 1 LED is not here: it is fixed green and no command addresses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BiColourLed {
    /// The A button on the stick.
    ButtonA,
    /// The B button on the stick.
    ButtonB,
    /// The D button on the throttle.
    ButtonD,
    /// The E button on the throttle.
    ButtonE,
    /// Toggle switch 1 (T1/T2) on the stick base.
    T1T2,
    /// Toggle switch 2 (T3/T4) on the stick base.
    T3T4,
    /// Toggle switch 3 (T5/T6) on the stick base.
    T5T6,
    /// The POV 2 hat on the stick head.
    Pov2,
    /// The i (clutch) button on the throttle.
    ButtonI,
}

impl BiColourLed {
    /// Every bi-colour LED, in the order of the LED bit table.
    pub const ALL: [Self; 9] = [
        Self::ButtonA,
        Self::ButtonB,
        Self::ButtonD,
        Self::ButtonE,
        Self::T1T2,
        Self::T3T4,
        Self::T5T6,
        Self::Pov2,
        Self::ButtonI,
    ];

    /// The `0xb8` bit of the red half.
    #[must_use]
    pub const fn red_bit(self) -> u8 {
        match self {
            Self::ButtonA => 2,
            Self::ButtonB => 4,
            Self::ButtonD => 6,
            Self::ButtonE => 8,
            Self::T1T2 => 10,
            Self::T3T4 => 12,
            Self::T5T6 => 14,
            Self::Pov2 => 16,
            Self::ButtonI => 18,
        }
    }

    /// The `0xb8` bit of the green half, the one after the red bit.
    #[must_use]
    pub const fn green_bit(self) -> u8 {
        self.red_bit() + 1
    }

    /// The key of this LED in a profile's `[leds]` table.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ButtonA => "a",
            Self::ButtonB => "b",
            Self::ButtonD => "d",
            Self::ButtonE => "e",
            Self::T1T2 => "t1_t2",
            Self::T3T4 => "t3_t4",
            Self::T5T6 => "t5_t6",
            Self::Pov2 => "pov_2",
            Self::ButtonI => "i",
        }
    }
}

/// A single-colour LED of the X52 Pro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SingleColourLed {
    /// The Fire button on the stick, under the flip cover.
    Fire,
    /// The throttle.
    Throttle,
}

impl SingleColourLed {
    /// Every single-colour LED, in the order of the LED bit table.
    pub const ALL: [Self; 2] = [Self::Fire, Self::Throttle];

    /// The `0xb8` bit.
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Fire => 1,
            Self::Throttle => 20,
        }
    }

    /// The key of this LED in a profile's `[leds]` table.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Throttle => "throttle",
        }
    }
}

/// The colour of a bi-colour LED. Amber is red and green together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LedColour {
    /// Both halves off.
    Off,
    /// The green half on.
    Green,
    /// The red half on.
    Red,
    /// Both halves on.
    Amber,
}

impl LedColour {
    /// Whether the green half is on.
    #[must_use]
    pub const fn green(self) -> bool {
        matches!(self, Self::Green | Self::Amber)
    }

    /// Whether the red half is on.
    #[must_use]
    pub const fn red(self) -> bool {
        matches!(self, Self::Red | Self::Amber)
    }
}

#[cfg(test)]
mod tests;
