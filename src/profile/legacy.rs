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

//! The one-time import of the GX52 0.7 profile database (docs/profiles.md, "Legacy import";
//! spike S8 for the schema).
//!
//! The original database is never opened or modified: a private copy is read. The import is
//! planned before anything is written: every row gets its target id and final name, and the plan
//! is saved next to the profile directory. A start that is interrupted between two writes resumes from the saved plan, so a
//! rerun gives the same profiles as one clean run, and a file that is already there never stands
//! in for a row unless it is the file the plan wrote.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use gettextrs::gettext;
use gtk::glib;
use rusqlite::types::{Value as Column, ValueRef};
use rusqlite::{Connection, OpenFlags};
use toml::{Table, Value};
use x52::clock::{Clock1Source, DateFormat, FixedOffset};
use x52::command::Brightness;
use x52::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};

use super::files::{self, Created, PROFILE_LIMIT};
use super::name;
use super::schema::{self, FieldProblem, ProblemKind};
use super::store::{BuiltinReason, Origin, ProfileStore};
use super::{Profile, ProfileId, ZoneSpec};

/// The largest database that is opened; the two S8 databases are 32 KiB each.
pub(crate) const DATABASE_LIMIT: u64 = 16 * 1024 * 1024;

/// The largest plan file that is read back.
const PLAN_LIMIT: u64 = 16 * 1024 * 1024;

/// GX52 0.7's brightness scale: `0`–`32`, sent multiplied by 4.
const LEGACY_BRIGHTNESS_MAX: i64 = 32;

/// The version of the plan file's own format.
const PLAN_VERSION: i64 = 1;

/// Where GX52 0.7 kept its database, for every build profile (docs/profiles.md, "Files").
pub(crate) fn database_path() -> PathBuf {
    glib::user_config_dir().join("gx52").join("gx52.db")
}

/// The plan file inside the app's config directory, next to `profiles/`.
pub(crate) fn plan_path(config_dir: &Path) -> PathBuf {
    config_dir.join("legacy-import.toml")
}

/// A profile table of the GX52 0.7 database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LegacyTable {
    /// `x52_pro_profile`: X52 Pro profiles, with the LED columns.
    X52Pro,
    /// `x52_profile`: X52 profiles; its LED columns (2019 schema) are never read.
    X52,
}

impl LegacyTable {
    const ALL: [Self; 2] = [Self::X52Pro, Self::X52];

    /// The table's name.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::X52Pro => "x52_pro_profile",
            Self::X52 => "x52_profile",
        }
    }

    const fn device_type(self) -> DeviceType {
        match self {
            Self::X52Pro => DeviceType::X52Pro,
            Self::X52 => DeviceType::X52,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|table| table.name() == name)
    }
}

/// Why the database could not be read. Every error but [`DatabaseError::Changing`] is final: the
/// import is marked done and not tried again.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DatabaseError {
    /// The database or its journal kept changing while it was read: GX52 0.7 is running and
    /// writing it. Not final; the import is tried again at the next start.
    #[error("the database kept changing while it was read")]
    Changing,
    /// The file is larger than [`DATABASE_LIMIT`].
    #[error("the database is larger than {DATABASE_LIMIT} bytes")]
    TooLarge,
    /// The file cannot be read.
    #[error("the database cannot be read: {0}")]
    Io(io::Error),
    /// SQLite refused the file or a query.
    #[error("the database cannot be read: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Neither profile table exists.
    #[error("the database has no GX52 profile table")]
    NoProfileTable,
}

impl DatabaseError {
    /// Whether the import is over after this error, so it is marked done.
    pub(crate) const fn is_final(&self) -> bool {
        match self {
            Self::Changing => false,
            Self::TooLarge | Self::Io(_) | Self::Sqlite(_) | Self::NoProfileTable => true,
        }
    }
}

/// The two GX52 0.7 settings the rewrite keeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LegacySettings {
    /// `settings_launch_on_login`: start GX52 when logging in.
    pub(crate) launch_on_login: Option<bool>,
    /// `settings_show_app_indicator`: show the tray icon.
    pub(crate) show_app_indicator: Option<bool>,
}

/// One profile row of the database, converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyRow {
    /// The table it comes from.
    pub(crate) table: LegacyTable,
    /// Its `id`.
    pub(crate) row_id: i64,
    /// Whether it is that table's default row (`can_be_removed = 0`).
    pub(crate) is_default: bool,
    /// The profile, with the type default for every value that was out of range.
    pub(crate) profile: Profile,
    /// The values that were out of range, by column.
    pub(crate) problems: Vec<FieldProblem>,
}

/// Everything the import takes from the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyDatabase {
    /// The profile rows: X52 Pro first, each table in `id` order.
    pub(crate) rows: Vec<LegacyRow>,
    /// The settings.
    pub(crate) settings: LegacySettings,
}

/// The files SQLite may keep next to a database: the rollback journal of an interrupted write,
/// and the write-ahead log.
const COMPANION_SUFFIXES: [&str; 2] = ["-journal", "-wal"];

/// How many times the files are read before the database counts as changing.
const SNAPSHOT_ATTEMPTS: usize = 5;

/// The pause between two reads that differ.
const SNAPSHOT_PAUSE: Duration = Duration::from_millis(200);

/// Reads the database at `path`, never opening or modifying the original.
///
/// The database and its journal or write-ahead log, when there is one, are read into memory (see
/// [`snapshot`]), written to a private temporary directory, and the copy is opened. A GX52 0.7
/// that stopped in the middle of a write leaves a hot journal that SQLite must roll back before
/// anything can be read, which a read-only open cannot do; the copy can, and a running GX52 0.7
/// never holds a lock on it.
///
/// # Errors
///
/// [`DatabaseError`] when a file is too large or cannot be read, the files keep changing, or the
/// copy cannot be read or has no profile table.
pub(crate) fn read_database(path: &Path) -> Result<LegacyDatabase, DatabaseError> {
    let files = snapshot(path, SNAPSHOT_PAUSE, &mut read_database_file)?;
    let copy = tempfile::Builder::new()
        .prefix("gx52-legacy-")
        .tempdir()
        .map_err(DatabaseError::Io)?;
    let copied = copy.path().join("gx52.db");
    fs::write(&copied, &files.database).map_err(DatabaseError::Io)?;
    for (suffix, contents) in COMPANION_SUFFIXES.iter().zip(&files.companions) {
        if let Some(contents) = contents {
            fs::write(companion_path(&copied, suffix), contents).map_err(DatabaseError::Io)?;
        }
    }
    let connection = Connection::open_with_flags(
        &copied,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut rows = Vec::new();
    let mut any_table = false;
    for table in LegacyTable::ALL {
        let Some(table_rows) = read_table(&connection, table)? else {
            continue;
        };
        any_table = true;
        rows.extend(convert_table(table, table_rows));
    }
    if !any_table {
        return Err(DatabaseError::NoProfileTable);
    }
    let settings = read_settings(&connection)?;
    Ok(LegacyDatabase { rows, settings })
}

/// The contents of the database and of its companion files, `None` for a companion that does not
/// exist.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub(crate) database: Vec<u8>,
    pub(crate) companions: [Option<Vec<u8>>; COMPANION_SUFFIXES.len()],
}

/// Reads the database and its companions through `read` until two reads in a row agree, up to
/// [`SNAPSHOT_ATTEMPTS`] times, pausing `pause` after a read that differs from the one before.
///
/// Files copied one after the other can come from two states: a rollback between the copy of the
/// database and the copy of its journal leaves uncommitted pages and no journal to undo them. Two
/// identical reads of the set mean that no file changed between its two reads, so the database
/// as read and every companion as read were all on disk together at the moment the first read
/// ended: one state SQLite can recover from, as from a crash.
///
/// # Errors
///
/// [`DatabaseError::Changing`] when no two reads in a row agree, or the error of `read`.
pub(crate) fn snapshot(
    path: &Path,
    pause: Duration,
    read: &mut impl FnMut(&Path) -> Result<Option<Vec<u8>>, DatabaseError>,
) -> Result<Snapshot, DatabaseError> {
    let mut previous = read_once(path, read)?;
    for _ in 1..SNAPSHOT_ATTEMPTS {
        let current = read_once(path, read)?;
        if current == previous {
            return Ok(current);
        }
        tracing::debug!(
            "{} changed while it was read; reading it again",
            path.display()
        );
        previous = current;
        thread::sleep(pause);
    }
    Err(DatabaseError::Changing)
}

fn read_once(
    path: &Path,
    read: &mut impl FnMut(&Path) -> Result<Option<Vec<u8>>, DatabaseError>,
) -> Result<Snapshot, DatabaseError> {
    let database = read(path)?.ok_or_else(|| {
        DatabaseError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "the database does not exist",
        ))
    })?;
    let mut companions = [None, None];
    for (suffix, contents) in COMPANION_SUFFIXES.iter().zip(&mut companions) {
        *contents = read(&companion_path(path, suffix))?;
    }
    Ok(Snapshot {
        database,
        companions,
    })
}

fn companion_path(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The contents of `path` when it is at most [`DATABASE_LIMIT`] bytes, `None` when it does not
/// exist.
fn read_database_file(path: &Path) -> Result<Option<Vec<u8>>, DatabaseError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DatabaseError::Io(error)),
    };
    let mut contents = Vec::new();
    file.take(DATABASE_LIMIT + 1)
        .read_to_end(&mut contents)
        .map_err(DatabaseError::Io)?;
    if u64::try_from(contents.len()).unwrap_or(u64::MAX) > DATABASE_LIMIT {
        return Err(DatabaseError::TooLarge);
    }
    Ok(Some(contents))
}

/// The rows of `table` by column name, in `id` order, or `None` when the table does not exist.
fn read_table(
    connection: &Connection,
    table: LegacyTable,
) -> rusqlite::Result<Option<Vec<BTreeMap<String, Column>>>> {
    if !table_exists(connection, table.name())? {
        return Ok(None);
    }
    // The table name is one of two constants, never input.
    let mut statement = connection.prepare(&format!(
        "SELECT * FROM \"{}\" ORDER BY \"id\"",
        table.name()
    ))?;
    let names: Vec<String> = statement.column_names().map(str::to_owned).collect();
    let mut query = statement.query([])?;
    let mut rows = Vec::new();
    while let Some(row) = query.next()? {
        let mut columns = BTreeMap::new();
        for (index, name) in names.iter().enumerate() {
            columns.insert(name.clone(), owned(row.get_ref(index)?));
        }
        rows.push(columns);
    }
    Ok(Some(rows))
}

fn table_exists(connection: &Connection, name: &str) -> rusqlite::Result<bool> {
    connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?
        .exists([name])
}

fn read_settings(connection: &Connection) -> rusqlite::Result<LegacySettings> {
    let mut settings = LegacySettings::default();
    if !table_exists(connection, "setting")? {
        return Ok(settings);
    }
    let mut statement = connection.prepare("SELECT \"key\", \"value\" FROM \"setting\"")?;
    let mut query = statement.query([])?;
    while let Some(row) = query.next()? {
        let Column::Text(key) = owned(row.get_ref(0)?) else {
            continue;
        };
        let value = flag(&owned(row.get_ref(1)?));
        match key.as_str() {
            "settings_launch_on_login" => settings.launch_on_login = value,
            "settings_show_app_indicator" => settings.show_app_indicator = value,
            _ => {}
        }
    }
    Ok(settings)
}

/// An owned copy of a column. Text that is not UTF-8 is converted lossily, so one corrupt value
/// costs that field, not the whole import.
fn owned(value: ValueRef<'_>) -> Column {
    match value {
        ValueRef::Null => Column::Null,
        ValueRef::Integer(value) => Column::Integer(value),
        ValueRef::Real(value) => Column::Real(value),
        ValueRef::Text(bytes) => Column::Text(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Blob(bytes) => Column::Blob(bytes.to_vec()),
    }
}

/// Converts a table's rows; the first row with `can_be_removed = 0` is the default.
fn convert_table(table: LegacyTable, rows: Vec<BTreeMap<String, Column>>) -> Vec<LegacyRow> {
    let mut default_taken = false;
    rows.into_iter()
        .filter_map(|columns| {
            let row_id = columns.get("id").and_then(integer)?;
            let removable = columns.get("can_be_removed").and_then(flag);
            let is_default = removable == Some(false) && !default_taken;
            default_taken |= is_default;
            let mut converter = Converter::default();
            let profile = converter.profile(table, &columns);
            Some(LegacyRow {
                table,
                row_id,
                is_default,
                profile,
                problems: converter.problems,
            })
        })
        .collect()
}

/// An integer column: SQLite integers, and text holding one (GX52 0.7 stored its enums as
/// text digits).
fn integer(column: &Column) -> Option<i64> {
    match column {
        Column::Integer(value) => Some(*value),
        Column::Text(text) => text.trim().parse().ok(),
        Column::Blob(bytes) => std::str::from_utf8(bytes).ok()?.trim().parse().ok(),
        Column::Null | Column::Real(_) => None,
    }
}

/// A `0` / `1` column.
fn flag(column: &Column) -> Option<bool> {
    match integer(column)? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Converts the columns of one row, collecting the values that had to be replaced.
#[derive(Debug, Default)]
struct Converter {
    problems: Vec<FieldProblem>,
}

impl Converter {
    fn profile(&mut self, table: LegacyTable, columns: &BTreeMap<String, Column>) -> Profile {
        let device_type = table.device_type();
        let raw_name = match columns.get("name") {
            Some(Column::Text(text)) => text.clone(),
            _ => String::new(),
        };
        let name = name::truncate(&raw_name).unwrap_or_else(|| {
            self.problem("name", ProblemKind::Name(name::NameProblem::Empty));
            imported_name()
        });
        if raw_name.trim().chars().count() > name::MAX_CHARS {
            let count = raw_name.trim().chars().count();
            self.problem("name", ProblemKind::Name(name::NameProblem::TooLong(count)));
        }
        let mut profile = Profile::builtin(device_type, name);
        for (column, field) in [
            ("led_brightness", &mut profile.led_brightness),
            ("mfd_brightness", &mut profile.mfd_brightness),
        ] {
            if let Some(level) = self.mapped(columns, column, legacy_brightness) {
                *field = level;
            }
        }
        if let Some(local) = self.mapped(columns, "clock_1_use_local_time", bool_digit) {
            profile.clock1.source = if local {
                Clock1Source::Local
            } else {
                Clock1Source::Utc
            };
        }
        if let Some(h24) = self.mapped(columns, "clock_1_use_24h", bool_digit) {
            profile.clock1.h24 = h24;
        }
        for (prefix, clock) in [
            ("clock_2", &mut profile.clock2),
            ("clock_3", &mut profile.clock3),
        ] {
            let offset_column = format!("{prefix}_offset");
            if let Some(offset) = self.mapped(columns, &offset_column, legacy_offset) {
                clock.source = ZoneSpec::Fixed(offset);
            }
            if let Some(h24) = self.mapped(columns, &format!("{prefix}_use_24h"), bool_digit) {
                clock.h24 = h24;
            }
        }
        if let Some(format) = self.mapped(columns, "date_format", legacy_date_format) {
            profile.date_format = format;
        }
        // LED columns only from the X52 Pro table: the 2019 schema's x52_profile has them too,
        // but the X52 has no LED control.
        if table == LegacyTable::X52Pro {
            for led in SingleColourLed::ALL {
                let column = format!("led_{}", led.key());
                if let Some(on) = self.mapped(columns, &column, bool_digit) {
                    profile.leds.set_on(led, on);
                }
            }
            for led in BiColourLed::ALL {
                let column = format!("led_{}", led.key());
                if let Some(colour) = self.mapped(columns, &column, legacy_colour) {
                    profile.leds.set_colour(led, colour);
                }
            }
        }
        profile
    }

    /// The value of `column` through `convert`, or `None` with a problem (the built-in value is
    /// kept).
    fn mapped<T>(
        &mut self,
        columns: &BTreeMap<String, Column>,
        column: &str,
        convert: impl FnOnce(i64) -> Option<T>,
    ) -> Option<T> {
        let Some(raw) = columns.get(column) else {
            self.problem(column, ProblemKind::Missing);
            return None;
        };
        let converted = integer(raw).and_then(convert);
        if converted.is_none() {
            self.problem(column, ProblemKind::UnknownValue(describe(raw)));
        }
        converted
    }

    fn problem(&mut self, key: &str, kind: ProblemKind) {
        self.problems.push(FieldProblem {
            key: key.to_owned(),
            kind,
        });
    }
}

/// The name of an imported profile whose name is empty.
fn imported_name() -> String {
    gettext("Imported profile")
}

fn describe(column: &Column) -> String {
    match column {
        Column::Null => "NULL".to_owned(),
        Column::Integer(value) => value.to_string(),
        Column::Real(value) => value.to_string(),
        Column::Text(text) => format!("{text:?}"),
        Column::Blob(bytes) => format!("a {}-byte blob", bytes.len()),
    }
}

fn legacy_brightness(level: i64) -> Option<Brightness> {
    if !(0..=LEGACY_BRIGHTNESS_MAX).contains(&level) {
        return None;
    }
    u8::try_from(level * 4)
        .ok()
        .and_then(|wire| Brightness::new(wire).ok())
}

fn legacy_offset(minutes: i64) -> Option<FixedOffset> {
    i16::try_from(minutes)
        .ok()
        .and_then(|minutes| FixedOffset::new(minutes).ok())
}

fn bool_digit(value: i64) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn legacy_date_format(value: i64) -> Option<DateFormat> {
    match value {
        0 => Some(DateFormat::YyMmDd),
        1 => Some(DateFormat::DdMmYy),
        2 => Some(DateFormat::MmDdYy),
        _ => None,
    }
}

fn legacy_colour(value: i64) -> Option<LedColour> {
    match value {
        0 => Some(LedColour::Off),
        1 => Some(LedColour::Green),
        2 => Some(LedColour::Red),
        3 => Some(LedColour::Amber),
        _ => None,
    }
}

/// One row of the plan: where it goes and what is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanEntry {
    /// The source table.
    pub(crate) table: LegacyTable,
    /// The source row's `id`.
    pub(crate) row_id: i64,
    /// The profile id the row is written under.
    pub(crate) target: ProfileId,
    /// The profile's name, made unique within its type.
    pub(crate) name: String,
    /// The file's contents.
    pub(crate) contents: String,
    /// The values that were replaced, in the user's language, for the report.
    pub(crate) problems: Vec<String>,
}

/// The whole import, decided before the first profile is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Every row, in the order it is written.
    pub(crate) entries: Vec<PlanEntry>,
    /// The settings to apply once every row is written.
    pub(crate) settings: LegacySettings,
    /// Every entry is written or skipped: the import is over even if the done flag is lost.
    pub(crate) completed: bool,
}

/// Decides where every row of `database` goes, against the profiles already in `store`.
///
/// A default row becomes its type's default profile when that profile's file does not exist yet;
/// otherwise, and for every other row, it becomes a new profile with a new random id. Names are
/// made unique within the type, in row order.
pub(crate) fn plan(database: &LegacyDatabase, store: &ProfileStore) -> Plan {
    let mut taken: Vec<(DeviceType, String)> = store
        .entries()
        .filter_map(|(_, entry)| Some((entry.device_type(), entry.name()?.to_owned())))
        .collect();
    let mut entries = Vec::new();
    for row in &database.rows {
        let device_type = row.table.device_type();
        let default_id = ProfileId::default_for(device_type);
        let default_free = row.is_default
            && store
                .loaded(&default_id)
                .is_some_and(|loaded| loaded.origin == Origin::Builtin(BuiltinReason::Missing))
            && !entries
                .iter()
                .any(|entry: &PlanEntry| entry.target == default_id);
        let mut profile = row.profile.clone();
        let target = if default_free {
            // The default keeps its id and its name; only the values come from GX52 0.7.
            profile.name = store.loaded(&default_id).map_or_else(
                || profile.name.clone(),
                |loaded| loaded.profile.name.clone(),
            );
            default_id
        } else {
            profile.name = name::unique(&profile.name, |candidate| {
                taken.contains(&(device_type, candidate.to_owned()))
            });
            ProfileId::random()
        };
        taken.push((device_type, profile.name.clone()));
        entries.push(PlanEntry {
            table: row.table,
            row_id: row.row_id,
            target,
            name: profile.name.clone(),
            contents: schema::write(&profile),
            problems: row.problems.iter().map(FieldProblem::message).collect(),
        });
    }
    Plan {
        entries,
        settings: database.settings,
        completed: false,
    }
}

impl Plan {
    /// The plan file.
    pub(crate) fn to_toml(&self) -> String {
        let mut out = format!("version = {PLAN_VERSION}\ncompleted = {}\n", self.completed);
        for (key, value) in [
            ("launch_on_login", self.settings.launch_on_login),
            ("show_app_indicator", self.settings.show_app_indicator),
        ] {
            if let Some(value) = value {
                let _ = writeln!(out, "{key} = {value}");
            }
        }
        for entry in &self.entries {
            let problems: Vec<String> = entry
                .problems
                .iter()
                .map(|text| toml_string(text))
                .collect();
            let _ = write!(
                out,
                "\n[[entry]]\ntable = {}\nrow_id = {}\nid = {}\nname = {}\nproblems = [{}]\ncontents = {}\n",
                toml_string(entry.table.name()),
                entry.row_id,
                toml_string(entry.target.as_str()),
                toml_string(&entry.name),
                problems.join(", "),
                toml_string(&entry.contents),
            );
        }
        out
    }

    /// The plan in `text`, or `None` when it is not a plan this app wrote.
    pub(crate) fn from_toml(text: &str) -> Option<Self> {
        let document: Table = text.parse().ok()?;
        if document.get("version")?.as_integer()? != PLAN_VERSION {
            return None;
        }
        let setting = |key| document.get(key).and_then(Value::as_bool);
        let settings = LegacySettings {
            launch_on_login: setting("launch_on_login"),
            show_app_indicator: setting("show_app_indicator"),
        };
        let entries = match document.get("entry") {
            None => Vec::new(),
            Some(entries) => entries
                .as_array()?
                .iter()
                .map(plan_entry)
                .collect::<Option<Vec<_>>>()?,
        };
        let completed = document
            .get("completed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Some(Self {
            entries,
            settings,
            completed,
        })
    }
}

fn plan_entry(value: &Value) -> Option<PlanEntry> {
    let table = value.as_table()?;
    let text = |key| table.get(key).and_then(Value::as_str);
    Some(PlanEntry {
        table: LegacyTable::from_name(text("table")?)?,
        row_id: table.get("row_id")?.as_integer()?,
        target: ProfileId::parse(text("id")?)?,
        name: text("name")?.to_owned(),
        contents: text("contents")?.to_owned(),
        problems: table
            .get("problems")?
            .as_array()?
            .iter()
            .map(|problem| problem.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?,
    })
}

fn toml_string(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// How the import writes files: [`FileWriter`] in the app, a fake that fails on demand in the
/// tests.
pub(crate) trait ProfileWriter {
    /// Creates `path` with `contents`, never over an existing file.
    ///
    /// # Errors
    ///
    /// The I/O error of the write.
    fn create_new(&mut self, path: &Path, contents: &[u8]) -> io::Result<Created>;

    /// Writes `contents` over `path` atomically.
    ///
    /// # Errors
    ///
    /// The I/O error of the write.
    fn replace(&mut self, path: &Path, contents: &[u8]) -> io::Result<()>;
}

/// The [`ProfileWriter`] of the real file system.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FileWriter;

impl ProfileWriter for FileWriter {
    fn create_new(&mut self, path: &Path, contents: &[u8]) -> io::Result<Created> {
        files::create_new(path, contents)
    }

    fn replace(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
        files::replace(path, contents)
    }
}

/// A plan entry whose target file holds something else than the plan wrote: kept, not imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conflict {
    /// The source table.
    pub(crate) table: LegacyTable,
    /// The source row's `id`.
    pub(crate) row_id: i64,
    /// The profile id whose file was kept.
    pub(crate) target: ProfileId,
}

/// What a completed import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ImportReport {
    /// Every profile the import produced (written now or by an earlier, interrupted start).
    pub(crate) imported: Vec<ProfileId>,
    /// Rows whose target file was changed by someone else since the plan was made.
    pub(crate) conflicts: Vec<Conflict>,
    /// The values that were replaced, as `<profile name>: <problem>`.
    pub(crate) problems: Vec<String>,
}

/// Why the import could not finish; the next start resumes from the saved plan.
#[derive(Debug, thiserror::Error)]
#[error("{context}: {source}")]
pub(crate) struct WriteError {
    context: String,
    source: io::Error,
}

/// Saves `plan` (unless it is already saved) and writes every entry that is not written yet.
///
/// # Errors
///
/// [`WriteError`] when the plan or a profile cannot be written; nothing that was written is
/// undone, and a rerun with the same saved plan continues where this one stopped.
pub(crate) fn execute(
    plan: &Plan,
    plan_saved: bool,
    config_dir: &Path,
    profiles_dir: &Path,
    writer: &mut impl ProfileWriter,
) -> Result<ImportReport, WriteError> {
    fs::create_dir_all(profiles_dir).map_err(|source| WriteError {
        context: format!("cannot create {}", profiles_dir.display()),
        source,
    })?;
    if !plan_saved {
        let path = plan_path(config_dir);
        writer
            .replace(&path, plan.to_toml().as_bytes())
            .map_err(|source| WriteError {
                context: format!("cannot write {}", path.display()),
                source,
            })?;
    }
    let mut report = ImportReport::default();
    for entry in &plan.entries {
        let path = profiles_dir.join(entry.target.file_name());
        let created = writer
            .create_new(&path, entry.contents.as_bytes())
            .map_err(|source| WriteError {
                context: format!("cannot write {}", path.display()),
                source,
            })?;
        let ours = match created {
            Created::Written => true,
            Created::AlreadyExists => files::read_limited(&path, PROFILE_LIMIT)
                .is_ok_and(|bytes| bytes == entry.contents.as_bytes()),
        };
        if ours {
            report.imported.push(entry.target.clone());
            report.problems.extend(
                entry
                    .problems
                    .iter()
                    .map(|problem| format!("{}: {problem}", entry.name)),
            );
        } else {
            report.conflicts.push(Conflict {
                table: entry.table,
                row_id: entry.row_id,
                target: entry.target.clone(),
            });
        }
    }
    Ok(report)
}

/// Saves `plan` again, marked completed, so a lost done flag never runs the import a second time.
/// A failure is only logged: the done flag is still set.
pub(crate) fn mark_completed(plan: &Plan, config_dir: &Path, writer: &mut impl ProfileWriter) {
    let completed = Plan {
        completed: true,
        ..plan.clone()
    };
    let path = plan_path(config_dir);
    if let Err(error) = writer.replace(&path, completed.to_toml().as_bytes()) {
        tracing::warn!("Cannot mark {} as completed: {error}", path.display());
    }
}

/// The saved plan in `config_dir`, if there is one this app can use.
pub(crate) fn saved_plan(config_dir: &Path) -> Option<Plan> {
    let path = plan_path(config_dir);
    let bytes = match files::read_limited(&path, PLAN_LIMIT) {
        Ok(bytes) => bytes,
        Err(files::ReadError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            return None;
        }
        Err(error) => {
            tracing::warn!("Legacy import plan {} not used: {error}", path.display());
            return None;
        }
    };
    let plan = std::str::from_utf8(&bytes).ok().and_then(Plan::from_toml);
    if plan.is_none() {
        tracing::warn!(
            "Legacy import plan {} is not readable; planning again",
            path.display()
        );
    }
    plan
}

/// The fixture databases are built here and shared with the start-up tests.
#[cfg(test)]
pub(crate) mod tests;
