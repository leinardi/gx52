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

//! Profiles: one full device state for one device type, stored as one TOML file each
//! (docs/profiles.md).
//!
//! Everything here is plain Rust. The functions that touch the disk block, so the app runs them
//! through `gio::spawn_blocking`; the tests call them directly on a temporary directory.

pub(crate) mod files;
pub(crate) mod migrate;
pub(crate) mod name;
pub(crate) mod schema;
pub(crate) mod store;

use std::fmt;
use std::path::{Path, PathBuf};

use gettextrs::gettext;
use gtk::glib;
use x52::charset::MfdText;
use x52::clock::{FixedOffset, ZoneSource};
use x52::command::{Brightness, MfdLine};
use x52::device::DeviceType;
use x52::state::{Clock1Settings, DeviceState, Leds, ZoneClockSettings};

use crate::config;

/// The number of MFD lines a profile holds.
pub(crate) const MFD_LINES: usize = MfdLine::ALL.len();

/// The id of a profile: the stem of its file name, stable for the profile's whole life.
///
/// Only lowercase ASCII letters, digits and `-`. The two default profiles have the reserved ids
/// `x52-pro-default` and `x52-default`; every other profile has a random lowercase UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ProfileId(String);

impl ProfileId {
    /// `text` as an id, or `None` when it has characters an id cannot have.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then(|| Self(text.to_owned()))
    }

    /// The reserved id of `device_type`'s default profile.
    pub(crate) fn default_for(device_type: DeviceType) -> Self {
        Self(format!("{}-default", device_type.id()))
    }

    /// A new random id, which can never be a reserved one.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "profiles added by the imports get one")
    )]
    pub(crate) fn random() -> Self {
        Self(glib::uuid_string_random().to_lowercase())
    }

    /// The device type whose default profile this id names, if it is a reserved id.
    pub(crate) fn default_type(&self) -> Option<DeviceType> {
        DeviceType::ALL
            .into_iter()
            .find(|device_type| *self == Self::default_for(*device_type))
    }

    /// The id as text.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the mode mapping compares ids with the GSettings strings"
        )
    )]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The name of this profile's file.
    pub(crate) fn file_name(&self) -> String {
        format!("{}.toml", self.0)
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The device type written as `id` in a profile (`x52-pro` or `x52`).
pub(crate) fn device_type_from_id(id: &str) -> Option<DeviceType> {
    DeviceType::ALL
        .into_iter()
        .find(|device_type| device_type.id() == id)
}

/// What clock 2 or 3 of a profile shows: a named zone, kept by name so it is written back as it
/// was read, or a fixed offset from UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ZoneSpec {
    /// An IANA time zone the system's database knows, for example `Europe/Rome`.
    Iana(String),
    /// A fixed offset from UTC, with no daylight saving time.
    Fixed(FixedOffset),
}

/// Clock 2 or clock 3 of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZoneClock {
    /// What the clock shows.
    pub(crate) source: ZoneSpec,
    /// 24 h when true, 12 h otherwise.
    pub(crate) h24: bool,
}

/// One profile: everything the device shows, for one device type.
///
/// Fields a device type lacks (the LEDs, shift and blink of the X52) are kept at their built-in
/// values and never written to that type's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Profile {
    /// The device type the profile is for.
    pub(crate) device_type: DeviceType,
    /// The name shown in the UI: 1–64 characters, trimmed, unique within the device type.
    pub(crate) name: String,
    /// The LED brightness.
    pub(crate) led_brightness: Brightness,
    /// The MFD backlight.
    pub(crate) mfd_brightness: Brightness,
    /// The SHIFT label; X52 Pro only.
    pub(crate) shift: bool,
    /// The i and POV 1 LEDs blink; X52 Pro only.
    pub(crate) blink: bool,
    /// The field order of the date.
    pub(crate) date_format: x52::clock::DateFormat,
    /// The LEDs; X52 Pro only.
    pub(crate) leds: Leds,
    /// The MFD lines as typed; each fits a line after conversion.
    pub(crate) mfd_lines: [String; MFD_LINES],
    /// Clock 1.
    pub(crate) clock1: Clock1Settings,
    /// Clock 2.
    pub(crate) clock2: ZoneClock,
    /// Clock 3.
    pub(crate) clock3: ZoneClock,
}

impl Profile {
    /// The built-in values of `device_type`'s default profile (docs/profiles.md, "Defaults"),
    /// named "Default".
    pub(crate) fn builtin_default(device_type: DeviceType) -> Self {
        Self::builtin(device_type, gettext("Default"))
    }

    /// The built-in values for `device_type` under `name`: the value a field falls back to when
    /// a file has a problem with it.
    pub(crate) fn builtin(device_type: DeviceType, name: String) -> Self {
        let state = DeviceState::default_for(device_type);
        Self {
            device_type,
            name,
            led_brightness: state.led_brightness,
            mfd_brightness: state.mfd_brightness,
            shift: state.shift,
            blink: state.blink,
            date_format: state.date_format,
            leds: state.leds,
            mfd_lines: Default::default(),
            clock1: state.clock1,
            clock2: ZoneClock::from_settings(&state.clock2),
            clock3: ZoneClock::from_settings(&state.clock3),
        }
    }

    /// The desired device state this profile stands for.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the mode mapping pushes the mapped profile's state"
        )
    )]
    pub(crate) fn to_device_state(&self) -> DeviceState {
        let builtin = DeviceState::default_for(self.device_type);
        let mfd_lines = self.mfd_lines.each_ref().map(|line| {
            MfdText::new(line).unwrap_or_else(|error| {
                tracing::warn!(
                    "Profile {:?}: MFD line {line:?} not shown: {error}",
                    self.name
                );
                MfdText::EMPTY
            })
        });
        DeviceState {
            mfd_brightness: self.mfd_brightness,
            led_brightness: self.led_brightness,
            leds: if self.device_type.has_leds() {
                self.leds
            } else {
                builtin.leds
            },
            mfd_lines,
            clock1: self.clock1,
            clock2: self.clock2.to_settings(&builtin.clock2),
            clock3: self.clock3.to_settings(&builtin.clock3),
            date_format: self.date_format,
            shift: self.device_type.has_shift() && self.shift,
            blink: self.device_type.has_blink() && self.blink,
        }
    }
}

impl ZoneClock {
    /// The profile form of a device-state clock.
    fn from_settings(settings: &ZoneClockSettings) -> Self {
        let source = match &settings.source {
            ZoneSource::Fixed(offset) => ZoneSpec::Fixed(*offset),
            ZoneSource::Zone(zone) => ZoneSpec::Iana(zone.iana_name().unwrap_or("UTC").to_owned()),
        };
        Self {
            source,
            h24: settings.h24,
        }
    }

    /// The device-state form, or `fallback` when the zone is no longer in the database.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the mode mapping pushes the mapped profile's state"
        )
    )]
    fn to_settings(&self, fallback: &ZoneClockSettings) -> ZoneClockSettings {
        let source = match &self.source {
            ZoneSpec::Fixed(offset) => ZoneSource::Fixed(*offset),
            ZoneSpec::Iana(name) => match ZoneSource::iana(name) {
                Ok(source) => source,
                Err(error) => {
                    tracing::warn!("Time zone {name:?} not shown: {error}");
                    fallback.source.clone()
                }
            },
        };
        ZoneClockSettings {
            source,
            h24: self.h24,
        }
    }
}

/// The directory of the app's own files: `gx52`, or `gx52-devel` for a Devel build so a host
/// Devel build never shares a production build's profiles (docs/profiles.md, "Files").
pub(crate) fn config_dir() -> PathBuf {
    let name = if config::is_devel() {
        "gx52-devel"
    } else {
        "gx52"
    };
    glib::user_config_dir().join(name)
}

/// The profile directory inside `config_dir`.
pub(crate) fn profiles_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("profiles")
}

#[cfg(test)]
mod tests;
