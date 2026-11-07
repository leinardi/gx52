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

//! The desired device state of docs/architecture.md ("State model") and the commands it stands
//! for.
//!
//! The state holds clock sources, not times: [`DeviceState::resolve`] turns it into the commands
//! that show it at a given instant. Fields a device type lacks (the LEDs, shift and blink of the
//! X52) are kept but never resolved into a command for that type.

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::charset::MfdText;
use crate::clock::{self, Clock1Source, ClockError, DateFormat, ZoneSource};
use crate::command::{Brightness, Command, MfdLine, OffsetClock};
use crate::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};

/// The states of the X52 Pro's LEDs, POV 1 excepted (it is fixed green).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Leds {
    /// Indexed by the position of the LED in [`BiColourLed::ALL`].
    colours: [LedColour; BiColourLed::ALL.len()],
    /// Indexed by the position of the LED in [`SingleColourLed::ALL`].
    switches: [bool; SingleColourLed::ALL.len()],
}

impl Leds {
    /// Every bi-colour LED in `colour` and every single-colour LED on when `on` is true.
    #[must_use]
    pub const fn uniform(colour: LedColour, on: bool) -> Self {
        Self {
            colours: [colour; BiColourLed::ALL.len()],
            switches: [on; SingleColourLed::ALL.len()],
        }
    }

    /// The colour of `led`.
    #[must_use]
    pub const fn colour(&self, led: BiColourLed) -> LedColour {
        self.colours[bi_colour_index(led)]
    }

    /// Sets the colour of `led`.
    pub const fn set_colour(&mut self, led: BiColourLed, colour: LedColour) {
        self.colours[bi_colour_index(led)] = colour;
    }

    /// Whether `led` is on.
    #[must_use]
    pub const fn is_on(&self, led: SingleColourLed) -> bool {
        self.switches[single_colour_index(led)]
    }

    /// Turns `led` on or off.
    pub const fn set_on(&mut self, led: SingleColourLed, on: bool) {
        self.switches[single_colour_index(led)] = on;
    }
}

/// The position of `led` in [`BiColourLed::ALL`].
const fn bi_colour_index(led: BiColourLed) -> usize {
    match led {
        BiColourLed::ButtonA => 0,
        BiColourLed::ButtonB => 1,
        BiColourLed::ButtonD => 2,
        BiColourLed::ButtonE => 3,
        BiColourLed::T1T2 => 4,
        BiColourLed::T3T4 => 5,
        BiColourLed::T5T6 => 6,
        BiColourLed::Pov2 => 7,
        BiColourLed::ButtonI => 8,
    }
}

/// The position of `led` in [`SingleColourLed::ALL`].
const fn single_colour_index(led: SingleColourLed) -> usize {
    match led {
        SingleColourLed::Fire => 0,
        SingleColourLed::Throttle => 1,
    }
}

/// What clock 1 shows and in which format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Clock1Settings {
    /// Local time or UTC.
    pub source: Clock1Source,
    /// 24 h when true, 12 h with an AM/PM indicator otherwise.
    pub h24: bool,
}

/// What clock 2 or clock 3 shows and in which format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneClockSettings {
    /// A time zone or a fixed offset from UTC.
    pub source: ZoneSource,
    /// 24 h when true, 12 h with an AM/PM indicator otherwise.
    pub h24: bool,
}

/// The desired device state: what the device should show, with clock sources instead of times.
///
/// A plain value computed on the main thread and handed to the device writer, which resolves it
/// with [`DeviceState::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceState {
    /// The MFD backlight.
    pub mfd_brightness: Brightness,
    /// The LED brightness, POV 1 included.
    pub led_brightness: Brightness,
    /// The LEDs; X52 Pro only.
    pub leds: Leds,
    /// The three MFD lines, top to bottom.
    pub mfd_lines: [MfdText; MfdLine::ALL.len()],
    /// Clock 1.
    pub clock1: Clock1Settings,
    /// Clock 2.
    pub clock2: ZoneClockSettings,
    /// Clock 3.
    pub clock3: ZoneClockSettings,
    /// The field order of the date.
    pub date_format: DateFormat,
    /// The SHIFT label on the MFD; X52 Pro only.
    pub shift: bool,
    /// The i and POV 1 LEDs blink; X52 Pro only.
    pub blink: bool,
}

/// The built-in values of docs/profiles.md ("Defaults").
mod defaults {
    use crate::clock::FixedOffset;

    /// Clock 2's fixed offset from UTC: UTC−04:00.
    pub(super) const CLOCK_2_OFFSET: FixedOffset = fixed(-240);
    /// Clock 3's fixed offset from UTC: UTC+08:00.
    pub(super) const CLOCK_3_OFFSET: FixedOffset = fixed(480);

    /// A fixed offset checked at compile time.
    const fn fixed(minutes: i16) -> FixedOffset {
        match FixedOffset::checked(minutes) {
            Some(offset) => offset,
            None => panic!("a default fixed offset is out of range"),
        }
    }
}

impl DeviceState {
    /// The built-in values of `device`'s default profile (docs/profiles.md, "Defaults"): both
    /// brightness values `128`; on the X52 Pro every bi-colour LED green and Fire and Throttle on
    /// (all off on the X52, which has no LED control); clock 1 local, 24 h; clock 2 at UTC−04:00
    /// and clock 3 at UTC+08:00, both fixed and 24 h; the date `yy-mm-dd`; shift and blink off;
    /// the MFD lines empty.
    #[must_use]
    pub fn default_for(device: DeviceType) -> Self {
        let fixed = |offset| ZoneClockSettings {
            source: ZoneSource::Fixed(offset),
            h24: true,
        };
        let leds = if device.has_leds() {
            Leds::uniform(LedColour::Green, true)
        } else {
            Leds::uniform(LedColour::Off, false)
        };
        Self {
            mfd_brightness: Brightness::FULL,
            led_brightness: Brightness::FULL,
            leds,
            mfd_lines: [MfdText::EMPTY; MfdLine::ALL.len()],
            clock1: Clock1Settings {
                source: Clock1Source::Local,
                h24: true,
            },
            clock2: fixed(defaults::CLOCK_2_OFFSET),
            clock3: fixed(defaults::CLOCK_3_OFFSET),
            date_format: DateFormat::YyMmDd,
            shift: false,
            blink: false,
        }
    }

    /// Every command that makes `device` show this state at `now`, with `local` as the system
    /// time zone, in the replay order of docs/architecture.md ("Threads and data flow"):
    /// brightness, LEDs, MFD lines, clocks, date, shift, blink.
    ///
    /// Only commands [supported](Command::supported_by) by `device` are returned, and at most one
    /// per [`CommandSlot`](crate::command::CommandSlot). This is not a diff against an earlier
    /// state: the writer compares the result with what it last transmitted.
    ///
    /// # Errors
    ///
    /// [`ClockError`] when a clock value cannot be computed at `now` (see
    /// [`clock::clock1_command`], [`clock::offset_command`] and [`clock::date_command`]).
    pub fn resolve(
        &self,
        device: DeviceType,
        now: Timestamp,
        local: &TimeZone,
    ) -> Result<Vec<Command>, ClockError> {
        let clock1 = self.clock1.source;
        let mut commands = vec![
            Command::MfdBrightness(self.mfd_brightness),
            Command::LedBrightness(self.led_brightness),
        ];
        commands.extend(
            BiColourLed::ALL
                .iter()
                .map(|led| Command::Led(*led, self.leds.colour(*led))),
        );
        commands.extend(
            SingleColourLed::ALL
                .iter()
                .map(|led| Command::LedSwitch(*led, self.leds.is_on(*led))),
        );
        commands.extend(
            MfdLine::ALL
                .iter()
                .zip(self.mfd_lines)
                .map(|(line, text)| Command::MfdText(*line, text)),
        );
        commands.push(clock::clock1_command(now, local, clock1, self.clock1.h24)?);
        for (clock, settings) in [
            (OffsetClock::Clock2, &self.clock2),
            (OffsetClock::Clock3, &self.clock3),
        ] {
            commands.push(clock::offset_command(
                now,
                local,
                clock1,
                clock,
                &settings.source,
                settings.h24,
            )?);
        }
        commands.push(clock::date_command(now, local, clock1, self.date_format)?);
        commands.push(Command::Shift(self.shift));
        commands.push(Command::Blink(self.blink));
        commands.retain(|command| command.supported_by(device));
        Ok(commands)
    }
}

#[cfg(test)]
mod tests;
