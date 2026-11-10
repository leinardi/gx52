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

//! The TOML schema of a profile file (docs/profiles.md, "TOML schema (version 1)").
//!
//! Reading takes two steps, so one bad field does not lose the whole profile: the text is parsed
//! into a [`Table`], `schema_version`, `device_type` and `kind` are read first, then every field is
//! converted on its own. A field with a problem gets its built-in value and the problem is listed.
//! Writing produces the layout of docs/profiles.md, with every key the device type has.

use std::fmt::{self, Write as _};

use gettextrs::gettext;
use toml::{Table, Value};
use x52::charset::MfdText;
use x52::clock::{Clock1Source, DateFormat, FixedOffset};
use x52::command::Brightness;
use x52::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};
use x52::state::Leds;

use super::migrate::{Schema, Version};
use super::name::{self, NameProblem};
use super::{
    IanaZone, MFD_LINES, MfdLineSource, ModeSets, ModeSettings, Profile, ProfileKind, ZoneClock,
    ZoneSpec, device_type_from_id,
};

/// Why a document is not a profile at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseError {
    /// The bytes are not UTF-8.
    #[error("the file is not UTF-8 text")]
    NotUtf8,
    /// The text is not TOML.
    #[error("the file is not valid TOML: {0}")]
    Malformed(String),
    /// `schema_version` is missing.
    #[error("schema_version is missing")]
    MissingSchemaVersion,
    /// `schema_version` is not a version number (an integer from 1).
    #[error("schema_version {0} is not a schema version")]
    WrongSchemaVersion(String),
    /// `device_type` is missing.
    #[error("device_type is missing")]
    MissingDeviceType,
    /// `device_type` names no supported device type.
    #[error("device_type {0} is not a supported device type")]
    UnknownDeviceType(String),
}

/// A document that could be read, completely or in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Parsed {
    /// A profile of the supported schema version, or migrated up to it.
    Readable {
        /// The profile, with built-in values for the fields that had problems.
        profile: Box<Profile>,
        /// What was wrong, field by field.
        problems: Vec<FieldProblem>,
        /// The file's version, when it was older and has been migrated.
        migrated_from: Option<i64>,
    },
    /// Made by a newer app: only listed, never edited, applied or written.
    Newer {
        /// The file's schema version.
        version: i64,
        /// The device type the file is for.
        device_type: DeviceType,
        /// The name, when it can be read.
        name: Option<String>,
    },
}

/// One field of a document that could not be used as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldProblem {
    /// The key, with its table: `kind`, `mode_2.leds.a`, `clock_2.source`,
    /// `settings.mfd.lines[1].text`.
    pub(crate) key: String,
    /// What is wrong with it.
    pub(crate) kind: ProblemKind,
}

/// What is wrong with a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProblemKind {
    /// The key is missing; the built-in value is used.
    Missing,
    /// The value has the wrong TOML type; the built-in value is used.
    WrongType {
        /// The type the key needs.
        expected: &'static str,
    },
    /// The value is outside its range; the built-in value is used.
    OutOfRange(String),
    /// The value is none of the allowed ones; the built-in value is used.
    UnknownValue(String),
    /// The name cannot be used; the built-in name is used.
    Name(NameProblem),
    /// The key is not in the schema; it is ignored.
    UnknownKey,
    /// The key exists, but not for this device type; it is ignored.
    NotForDeviceType,
    /// Another profile of the type has the same name; this one is shown under `to`.
    DuplicateName {
        /// The name the profile is shown under.
        to: String,
    },
}

impl fmt::Display for FieldProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = &self.key;
        match &self.kind {
            ProblemKind::Missing => write!(formatter, "{key} is missing"),
            ProblemKind::WrongType { expected } => write!(formatter, "{key} is not {expected}"),
            ProblemKind::OutOfRange(detail) => write!(formatter, "{key} is out of range: {detail}"),
            ProblemKind::UnknownValue(value) => {
                write!(formatter, "{key} has an unknown value {value}")
            }
            ProblemKind::Name(problem) => write!(formatter, "{key}: {problem}"),
            ProblemKind::UnknownKey => write!(formatter, "{key} is not a profile key"),
            ProblemKind::NotForDeviceType => {
                write!(formatter, "{key} does not apply to this device type")
            }
            ProblemKind::DuplicateName { to } => {
                write!(
                    formatter,
                    "{key} is used by another profile; shown as {to:?}"
                )
            }
        }
    }
}

impl FieldProblem {
    /// The problem in the user's language, for the UI; [`fmt::Display`] is for the log.
    pub(crate) fn message(&self) -> String {
        let text = match &self.kind {
            ProblemKind::Missing => gettext("{key} is missing; the default is used"),
            ProblemKind::WrongType { .. } | ProblemKind::UnknownValue(_) => {
                gettext("{key} has a value that is not valid; the default is used")
            }
            ProblemKind::OutOfRange(_) => gettext("{key} is out of range; the default is used"),
            ProblemKind::Name(_) => {
                gettext("The name is empty or too long; a default name is used")
            }
            ProblemKind::UnknownKey => gettext("{key} is not a profile setting and is ignored"),
            ProblemKind::NotForDeviceType => {
                gettext("{key} does not apply to this joystick and is ignored")
            }
            ProblemKind::DuplicateName { to } => {
                gettext("The name is already used; it is shown as “{name}”").replace("{name}", to)
            }
        };
        text.replace("{key}", &self.key)
    }
}

/// The keys of the top-level table, in the order they are written.
const TOP_LEVEL_KEYS: [&str; 12] = [
    "schema_version",
    "device_type",
    "name",
    "kind",
    "date_format",
    "clock_1",
    "clock_2",
    "clock_3",
    SINGLE_TABLE,
    MODE_TABLES[0],
    MODE_TABLES[1],
    MODE_TABLES[2],
];

/// The table of a single profile's mode settings.
const SINGLE_TABLE: &str = "settings";

/// The tables of a per-mode profile's mode settings, mode 1 to 3.
const MODE_TABLES: [&str; 3] = ["mode_1", "mode_2", "mode_3"];

/// The keys of a table of mode settings.
const SET_KEYS: [&str; 6] = [
    "led_brightness",
    "mfd_brightness",
    "shift",
    "blink",
    "leds",
    "mfd",
];

/// The device type a file names, read leniently: the file only has to be TOML with a known
/// `device_type` string. Used to list a damaged file under its type; `None` when that cannot be
/// read (not UTF-8, not TOML, no or an unknown `device_type`).
pub(crate) fn sniff_device_type(bytes: &[u8]) -> Option<DeviceType> {
    let document: Table = std::str::from_utf8(bytes).ok()?.parse().ok()?;
    device_type_from_id(document.get("device_type")?.as_str()?)
}

/// Parses `bytes` as a profile of `schema`, migrating an older version in memory.
///
/// # Errors
///
/// [`ParseError`] when the bytes are not a profile at all: not UTF-8, not TOML, or without a valid
/// `schema_version` and `device_type`. Every other problem is a [`FieldProblem`].
pub(crate) fn parse(bytes: &[u8], schema: Schema) -> Result<Parsed, ParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotUtf8)?;
    let mut document: Table = text
        .parse()
        .map_err(|error: toml::de::Error| ParseError::Malformed(error.message().to_owned()))?;
    let version = match document.get("schema_version") {
        None => return Err(ParseError::MissingSchemaVersion),
        Some(Value::Integer(version)) if *version >= 1 => *version,
        Some(other) => return Err(ParseError::WrongSchemaVersion(other.to_string())),
    };
    let device_type = match document.get("device_type") {
        None => return Err(ParseError::MissingDeviceType),
        Some(Value::String(id)) => {
            device_type_from_id(id).ok_or_else(|| ParseError::UnknownDeviceType(id.clone()))?
        }
        Some(other) => return Err(ParseError::UnknownDeviceType(other.to_string())),
    };
    match schema.migrate(&mut document, version) {
        Version::Newer(version) => Ok(Parsed::Newer {
            version,
            device_type,
            name: document
                .get("name")
                .and_then(Value::as_str)
                .and_then(|raw| name::normalise(raw).ok()),
        }),
        Version::Readable { migrated_from } => {
            let mut reader = Reader::default();
            let profile = reader.profile(&document, device_type);
            Ok(Parsed::Readable {
                profile: Box::new(profile),
                problems: reader.problems,
                migrated_from,
            })
        }
    }
}

/// The name a profile is shown under when its file has no usable one.
pub(crate) fn fallback_name() -> String {
    gettext("Unnamed profile")
}

/// Converts the fields of a document, collecting the problems.
#[derive(Debug, Default)]
struct Reader {
    problems: Vec<FieldProblem>,
}

impl Reader {
    fn profile(&mut self, document: &Table, device_type: DeviceType) -> Profile {
        let mut profile = Profile::builtin(device_type, fallback_name());
        self.unknown_keys(document, "", &TOP_LEVEL_KEYS);
        if let Some(raw) = self.string(document, "", "name") {
            match name::normalise(raw) {
                Ok(name) => profile.name = name,
                Err(problem) => self.problem("name", ProblemKind::Name(problem)),
            }
        }
        let kind = self.kind(document);
        if let Some(raw) = self.string(document, "", "date_format") {
            match date_format_from(raw) {
                Some(format) => profile.date_format = format,
                None => self.problem("date_format", ProblemKind::UnknownValue(quoted(raw))),
            }
        }
        self.clock1(document, &mut profile);
        for (key, clock) in [
            ("clock_2", &mut profile.clock2),
            ("clock_3", &mut profile.clock3),
        ] {
            self.zone_clock(document, key, clock);
        }
        profile.settings = match kind {
            ProfileKind::Single => {
                let settings = self.mode_settings(document, SINGLE_TABLE, device_type);
                self.other_kind_tables(document, &MODE_TABLES);
                ModeSets::Single(settings)
            }
            ProfileKind::PerMode => {
                let sets = MODE_TABLES.map(|key| self.mode_settings(document, key, device_type));
                self.other_kind_tables(document, &[SINGLE_TABLE]);
                ModeSets::PerMode(Box::new(sets))
            }
        };
        profile
    }

    /// The `kind`; a single profile when it is missing or unknown.
    fn kind(&mut self, document: &Table) -> ProfileKind {
        let Some(raw) = self.string(document, "", "kind") else {
            return ProfileKind::Single;
        };
        kind_from(raw).unwrap_or_else(|| {
            self.problem("kind", ProblemKind::UnknownValue(quoted(raw)));
            ProfileKind::Single
        })
    }

    /// Reports the mode tables of the other kind, which are ignored.
    fn other_kind_tables(&mut self, document: &Table, keys: &[&str]) {
        for key in keys {
            if document.contains_key(*key) {
                self.problem(key, ProblemKind::UnknownKey);
            }
        }
    }

    /// The mode settings in the table `key`; the built-in ones when it is missing.
    fn mode_settings(
        &mut self,
        document: &Table,
        key: &str,
        device_type: DeviceType,
    ) -> ModeSettings {
        let mut settings = ModeSettings::builtin(device_type);
        let Some(table) = self.table(document, "", key) else {
            return settings;
        };
        self.unknown_keys(table, key, &SET_KEYS);
        for (name, field) in [
            ("led_brightness", &mut settings.led_brightness),
            ("mfd_brightness", &mut settings.mfd_brightness),
        ] {
            if let Some(level) = self.brightness(table, key, name) {
                *field = level;
            }
        }
        for (name, supported, field) in [
            ("shift", device_type.has_shift(), &mut settings.shift),
            ("blink", device_type.has_blink(), &mut settings.blink),
        ] {
            if supported {
                if let Some(value) = self.boolean(table, key, name) {
                    *field = value;
                }
            } else if table.contains_key(name) {
                self.problem(&join(key, name), ProblemKind::NotForDeviceType);
            }
        }
        if device_type.has_leds() {
            self.leds(table, key, &mut settings.leds);
        } else if table.contains_key("leds") {
            self.problem(&join(key, "leds"), ProblemKind::NotForDeviceType);
        }
        self.mfd(table, key, &mut settings.mfd_lines);
        settings
    }

    fn brightness(&mut self, table: &Table, prefix: &str, key: &str) -> Option<Brightness> {
        let level = self.integer(table, prefix, key)?;
        let checked = u8::try_from(level)
            .ok()
            .and_then(|level| Brightness::new(level).ok());
        if checked.is_none() {
            self.problem(
                &join(prefix, key),
                ProblemKind::OutOfRange(format!("{level} is not in 0–{}", Brightness::MAX)),
            );
        }
        checked
    }

    fn leds(&mut self, set: &Table, prefix: &str, leds: &mut Leds) {
        let Some(table) = self.table(set, prefix, "leds") else {
            return;
        };
        let prefix = join(prefix, "leds");
        let known: Vec<&str> = SingleColourLed::ALL
            .iter()
            .map(|led| led.key())
            .chain(BiColourLed::ALL.iter().map(|led| led.key()))
            .collect();
        self.unknown_keys(table, &prefix, &known);
        for led in SingleColourLed::ALL {
            if let Some(raw) = self.string(table, &prefix, led.key()) {
                match switch_from(raw) {
                    Some(on) => leds.set_on(led, on),
                    None => self.problem(
                        &join(&prefix, led.key()),
                        ProblemKind::UnknownValue(quoted(raw)),
                    ),
                }
            }
        }
        for led in BiColourLed::ALL {
            if let Some(raw) = self.string(table, &prefix, led.key()) {
                match colour_from(raw) {
                    Some(colour) => leds.set_colour(led, colour),
                    None => self.problem(
                        &join(&prefix, led.key()),
                        ProblemKind::UnknownValue(quoted(raw)),
                    ),
                }
            }
        }
    }

    fn mfd(&mut self, set: &Table, prefix: &str, lines: &mut [MfdLineSource; MFD_LINES]) {
        let Some(mfd) = self.table(set, prefix, "mfd") else {
            return;
        };
        let prefix = join(prefix, "mfd");
        self.unknown_keys(mfd, &prefix, &["lines"]);
        let Some(value) = self.value(mfd, &prefix, "lines") else {
            return;
        };
        let Some(read) = value.as_array().filter(|read| read.len() == MFD_LINES) else {
            self.problem(
                &join(&prefix, "lines"),
                ProblemKind::WrongType {
                    expected: "an array of 3 tables",
                },
            );
            return;
        };
        for (index, (line, field)) in read.iter().zip(lines).enumerate() {
            let key = format!("{prefix}.lines[{index}]");
            let Some(table) = line.as_table() else {
                self.problem(
                    &key,
                    ProblemKind::WrongType {
                        expected: "a table",
                    },
                );
                continue;
            };
            if let Some(source) = self.line(table, &key) {
                *field = source;
            }
        }
    }

    /// One MFD line, `{ source = ... }`; `None` when it cannot be used.
    fn line(&mut self, table: &Table, key: &str) -> Option<MfdLineSource> {
        let raw = self.string(table, key, "source")?;
        let source = match raw {
            "text" => {
                self.unknown_keys(table, key, &["source", "text"]);
                let text = self.string(table, key, "text")?;
                if let Err(error) = MfdText::new(text) {
                    self.problem(
                        &join(key, "text"),
                        ProblemKind::OutOfRange(error.to_string()),
                    );
                    return None;
                }
                return Some(MfdLineSource::Text(text.to_owned()));
            }
            "empty" => MfdLineSource::Empty,
            "profile_name" => MfdLineSource::ProfileName,
            "last_button" => MfdLineSource::LastButton,
            _ => {
                self.problem(&join(key, "source"), ProblemKind::UnknownValue(quoted(raw)));
                return None;
            }
        };
        self.unknown_keys(table, key, &["source"]);
        Some(source)
    }

    fn clock1(&mut self, document: &Table, profile: &mut Profile) {
        let Some(clock) = self.table(document, "", "clock_1") else {
            return;
        };
        self.unknown_keys(clock, "clock_1", &["source", "use_24h"]);
        if let Some(raw) = self.string(clock, "clock_1", "source") {
            match raw {
                "local" => profile.clock1.source = Clock1Source::Local,
                "utc" => profile.clock1.source = Clock1Source::Utc,
                _ => self.problem("clock_1.source", ProblemKind::UnknownValue(quoted(raw))),
            }
        }
        if let Some(h24) = self.boolean(clock, "clock_1", "use_24h") {
            profile.clock1.h24 = h24;
        }
    }

    fn zone_clock(&mut self, document: &Table, key: &str, clock: &mut ZoneClock) {
        let Some(table) = self.table(document, "", key) else {
            return;
        };
        self.unknown_keys(table, key, &["source", "use_24h"]);
        if let Some(value) = self.value(table, key, "source") {
            match zone_spec_from(value) {
                Ok(source) => clock.source = source,
                Err(kind) => self.problem(&join(key, "source"), kind),
            }
        }
        if let Some(h24) = self.boolean(table, key, "use_24h") {
            clock.h24 = h24;
        }
    }

    /// The value of `key`, or `None` with a "missing" problem.
    fn value<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc Value> {
        let value = table.get(key);
        if value.is_none() {
            self.problem(&join(prefix, key), ProblemKind::Missing);
        }
        value
    }

    fn typed<'doc, T>(
        &mut self,
        table: &'doc Table,
        prefix: &str,
        key: &str,
        expected: &'static str,
        convert: impl FnOnce(&'doc Value) -> Option<T>,
    ) -> Option<T> {
        let converted = convert(self.value(table, prefix, key)?);
        if converted.is_none() {
            self.problem(&join(prefix, key), ProblemKind::WrongType { expected });
        }
        converted
    }

    fn table<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc Table> {
        self.typed(table, prefix, key, "a table", Value::as_table)
    }

    fn string<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc str> {
        self.typed(table, prefix, key, "a string", Value::as_str)
    }

    fn integer(&mut self, table: &Table, prefix: &str, key: &str) -> Option<i64> {
        self.typed(table, prefix, key, "an integer", Value::as_integer)
    }

    fn boolean(&mut self, table: &Table, prefix: &str, key: &str) -> Option<bool> {
        self.typed(table, prefix, key, "a boolean", Value::as_bool)
    }

    /// Reports every key of `table` that is not in `known`.
    fn unknown_keys(&mut self, table: &Table, prefix: &str, known: &[&str]) {
        for key in table.keys() {
            if !known.contains(&key.as_str()) {
                self.problem(&join(prefix, key), ProblemKind::UnknownKey);
            }
        }
    }

    fn problem(&mut self, key: &str, kind: ProblemKind) {
        self.problems.push(FieldProblem {
            key: key.to_owned(),
            kind,
        });
    }
}

/// `key` inside `prefix`, as a dotted path.
fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// A value as it appears in a problem report.
fn quoted(raw: &str) -> String {
    format!("{raw:?}")
}

/// A clock 2/3 source: `{ iana = "<zone>" }` or `{ fixed_offset_minutes = <n> }`.
fn zone_spec_from(value: &Value) -> Result<ZoneSpec, ProblemKind> {
    let unknown = || ProblemKind::UnknownValue(value.to_string());
    let table = value.as_table().ok_or_else(unknown)?;
    let mut entries = table.iter();
    let (Some((tag, inner)), None) = (entries.next(), entries.next()) else {
        return Err(unknown());
    };
    match (tag.as_str(), inner) {
        ("iana", Value::String(name)) => IanaZone::new(name)
            .map(ZoneSpec::Iana)
            .map_err(|_| ProblemKind::UnknownValue(quoted(name))),
        ("fixed_offset_minutes", Value::Integer(minutes)) => i16::try_from(*minutes)
            .ok()
            .and_then(|minutes| FixedOffset::new(minutes).ok())
            .map(ZoneSpec::Fixed)
            .ok_or_else(|| {
                ProblemKind::OutOfRange(format!(
                    "{minutes} is not in {}–{}",
                    FixedOffset::MIN_MINUTES,
                    FixedOffset::MAX_MINUTES
                ))
            }),
        _ => Err(unknown()),
    }
}

/// The text of a date format in a profile.
pub(crate) const fn date_format_text(format: DateFormat) -> &'static str {
    match format {
        DateFormat::DdMmYy => "dd-mm-yy",
        DateFormat::MmDdYy => "mm-dd-yy",
        DateFormat::YyMmDd => "yy-mm-dd",
    }
}

/// The text of a profile kind in a profile.
const fn kind_text(kind: ProfileKind) -> &'static str {
    match kind {
        ProfileKind::Single => "single",
        ProfileKind::PerMode => "per_mode",
    }
}

fn kind_from(raw: &str) -> Option<ProfileKind> {
    [ProfileKind::Single, ProfileKind::PerMode]
        .into_iter()
        .find(|kind| kind_text(*kind) == raw)
}

/// The TOML inline table of an MFD line.
fn line_table(line: &MfdLineSource) -> String {
    let source = match line {
        MfdLineSource::Text(text) => {
            return format!("{{ source = \"text\", text = {} }}", toml_string(text));
        }
        MfdLineSource::Empty => "empty",
        MfdLineSource::ProfileName => "profile_name",
        MfdLineSource::LastButton => "last_button",
    };
    format!("{{ source = {} }}", toml_string(source))
}

fn date_format_from(raw: &str) -> Option<DateFormat> {
    [DateFormat::DdMmYy, DateFormat::MmDdYy, DateFormat::YyMmDd]
        .into_iter()
        .find(|format| date_format_text(*format) == raw)
}

const fn switch_text(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn switch_from(raw: &str) -> Option<bool> {
    match raw {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

const fn colour_text(colour: LedColour) -> &'static str {
    match colour {
        LedColour::Off => "off",
        LedColour::Green => "green",
        LedColour::Red => "red",
        LedColour::Amber => "amber",
    }
}

fn colour_from(raw: &str) -> Option<LedColour> {
    [
        LedColour::Off,
        LedColour::Green,
        LedColour::Red,
        LedColour::Amber,
    ]
    .into_iter()
    .find(|colour| colour_text(*colour) == raw)
}

/// `text` as a TOML string, escaped.
fn toml_string(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// The file of `profile`, in the current schema, with every key its device type has.
pub(crate) fn write(profile: &Profile) -> String {
    let device_type = profile.device_type;
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = write_into(&mut out, profile, device_type);
    out
}

fn write_into(out: &mut String, profile: &Profile, device_type: DeviceType) -> fmt::Result {
    writeln!(out, "schema_version = {}", Schema::CURRENT.supported())?;
    writeln!(out, "device_type = {}", toml_string(device_type.id()))?;
    writeln!(out, "name = {}", toml_string(&profile.name))?;
    writeln!(out, "kind = {}", toml_string(kind_text(profile.kind())))?;
    writeln!(
        out,
        "date_format = {}",
        toml_string(date_format_text(profile.date_format))
    )?;
    let clock1 = match profile.clock1.source {
        Clock1Source::Local => "local",
        Clock1Source::Utc => "utc",
    };
    writeln!(out, "\n[clock_1]\nsource = {}", toml_string(clock1))?;
    writeln!(out, "use_24h = {}", profile.clock1.h24)?;
    for (key, clock) in [("clock_2", &profile.clock2), ("clock_3", &profile.clock3)] {
        let source = match &clock.source {
            ZoneSpec::Iana(zone) => format!("{{ iana = {} }}", toml_string(zone.name())),
            ZoneSpec::Fixed(offset) => {
                format!("{{ fixed_offset_minutes = {} }}", offset.minutes())
            }
        };
        writeln!(out, "\n[{key}]\nsource = {source}\nuse_24h = {}", clock.h24)?;
    }
    match &profile.settings {
        ModeSets::Single(settings) => write_set(out, SINGLE_TABLE, settings, device_type)?,
        ModeSets::PerMode(sets) => {
            for (key, settings) in MODE_TABLES.iter().zip(sets.iter()) {
                write_set(out, key, settings, device_type)?;
            }
        }
    }
    Ok(())
}

/// The table `key` of one set of mode settings, with its `leds` and `mfd` tables.
fn write_set(
    out: &mut String,
    key: &str,
    settings: &ModeSettings,
    device_type: DeviceType,
) -> fmt::Result {
    writeln!(out, "\n[{key}]")?;
    writeln!(out, "led_brightness = {}", settings.led_brightness.level())?;
    writeln!(out, "mfd_brightness = {}", settings.mfd_brightness.level())?;
    if device_type.has_shift() {
        writeln!(out, "shift = {}", settings.shift)?;
    }
    if device_type.has_blink() {
        writeln!(out, "blink = {}", settings.blink)?;
    }
    if device_type.has_leds() {
        writeln!(out, "\n[{key}.leds]")?;
        for led in SingleColourLed::ALL {
            let state = switch_text(settings.leds.is_on(led));
            writeln!(out, "{} = {}", led.key(), toml_string(state))?;
        }
        for led in BiColourLed::ALL {
            let colour = colour_text(settings.leds.colour(led));
            writeln!(out, "{} = {}", led.key(), toml_string(colour))?;
        }
    }
    writeln!(out, "\n[{key}.mfd]\nlines = [")?;
    for line in &settings.mfd_lines {
        writeln!(out, "  {},", line_table(line))?;
    }
    writeln!(out, "]")
}

#[cfg(test)]
mod tests;
