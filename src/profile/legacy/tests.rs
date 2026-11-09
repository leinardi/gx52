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

//! The legacy import's tests, and the fixture databases of both S8 schemas, which the start-up
//! tests share.

use std::fs::{self, File};

use rusqlite::params_from_iter;

use super::*;
use crate::profile::migrate::Schema;
use crate::profile::schema::Parsed;
use crate::profile::store::DefaultPersistence;

/// The two database schemas of spike S8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacySchema {
    /// The 2019 development version (the Flatpak database): `x52_profile` has LED columns too.
    Dev2019,
    /// The released schema (the source-install database): `x52_profile` has no LED columns.
    Release,
}

const LED_COLUMNS: [&str; 11] = [
    "led_fire",
    "led_a",
    "led_b",
    "led_d",
    "led_e",
    "led_t1_t2",
    "led_t3_t4",
    "led_t5_t6",
    "led_pov_2",
    "led_i",
    "led_throttle",
];

const COMMON_COLUMNS: &str = "name VARCHAR(255) NOT NULL, \
    led_brightness INTEGER NOT NULL, mfd_brightness INTEGER NOT NULL, \
    clock_1_use_local_time INTEGER NOT NULL, clock_1_use_24h INTEGER NOT NULL, \
    clock_2_offset INTEGER NOT NULL, clock_2_use_24h INTEGER NOT NULL, \
    clock_3_offset INTEGER NOT NULL, clock_3_use_24h INTEGER NOT NULL, \
    date_format VARCHAR(255) NOT NULL, can_be_removed INTEGER NOT NULL, \
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP";

/// An empty database of `schema`: the tables of GX52 0.7, as peewee created them.
pub(crate) fn create_database(path: &Path, schema: LegacySchema) -> Connection {
    let connection = Connection::open(path).unwrap();
    let leds: String = LED_COLUMNS
        .iter()
        .map(|column| format!("{column} VARCHAR(255) NOT NULL"))
        .collect::<Vec<_>>()
        .join(", ")
        + ", ";
    let x52_leds = if schema == LegacySchema::Dev2019 {
        leds.as_str()
    } else {
        ""
    };
    connection
        .execute_batch(&format!(
            "CREATE TABLE x52_pro_profile (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, \
                 {leds}{COMMON_COLUMNS});
             CREATE TABLE x52_profile (id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT, \
                 {x52_leds}{COMMON_COLUMNS});
             CREATE TABLE current_profile (id INTEGER NOT NULL PRIMARY KEY, profile_id INTEGER);
             CREATE TABLE setting (key VARCHAR(255) NOT NULL PRIMARY KEY, value BLOB NOT NULL);"
        ))
        .unwrap();
    connection
}

/// Inserts a row of `columns` into `table`.
pub(crate) fn insert(connection: &Connection, table: &str, columns: &[(&str, Column)]) {
    let names: Vec<&str> = columns.iter().map(|(name, _)| *name).collect();
    let placeholders: Vec<String> = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect();
    connection
        .execute(
            &format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                names.join(", "),
                placeholders.join(", ")
            ),
            params_from_iter(columns.iter().map(|(_, value)| value)),
        )
        .unwrap();
}

fn int(value: i64) -> Column {
    Column::Integer(value)
}

fn text(value: &str) -> Column {
    Column::Text(value.to_owned())
}

/// A profile row with the given values, LED columns from `leds` (in [`LED_COLUMNS`] order) when
/// given.
pub(crate) struct LegacyProfile<'values> {
    pub(crate) name: &'values str,
    pub(crate) brightness: (i64, i64),
    pub(crate) clock_1_local: i64,
    pub(crate) offsets: (i64, i64),
    pub(crate) use_24h: (i64, i64, i64),
    pub(crate) date_format: &'values str,
    pub(crate) can_be_removed: i64,
    pub(crate) leds: Option<[&'values str; 11]>,
}

impl LegacyProfile<'_> {
    pub(crate) fn insert(&self, connection: &Connection, table: LegacyTable) {
        let mut columns = vec![
            ("name", text(self.name)),
            ("led_brightness", int(self.brightness.0)),
            ("mfd_brightness", int(self.brightness.1)),
            ("clock_1_use_local_time", int(self.clock_1_local)),
            ("clock_1_use_24h", int(self.use_24h.0)),
            ("clock_2_offset", int(self.offsets.0)),
            ("clock_2_use_24h", int(self.use_24h.1)),
            ("clock_3_offset", int(self.offsets.1)),
            ("clock_3_use_24h", int(self.use_24h.2)),
            ("date_format", text(self.date_format)),
            ("can_be_removed", int(self.can_be_removed)),
        ];
        if let Some(leds) = self.leds {
            columns.extend(
                LED_COLUMNS
                    .iter()
                    .zip(leds)
                    .map(|(name, value)| (*name, text(value))),
            );
        }
        insert(connection, table.name(), &columns);
    }
}

/// GX52 0.7's defaults: what a fresh database held.
pub(crate) fn defaults(name: &str, can_be_removed: i64) -> LegacyProfile<'_> {
    LegacyProfile {
        name,
        brightness: (32, 32),
        clock_1_local: 1,
        offsets: (-240, 480),
        use_24h: (1, 1, 1),
        date_format: "0",
        can_be_removed,
        leds: Some(["1"; 11]),
    }
}

/// The Flatpak database of S8 (2019 schema): X52 Pro `Default` with mixed colours and dim
/// brightness, X52 Pro `Off`, X52 `Default` with LED columns that must be ignored. The X52 row
/// also carries the worked examples of profiles.md ("Clocks"): offsets `345` and `-240`, clock 1
/// on UTC.
pub(crate) fn flatpak_database(path: &Path) {
    let connection = create_database(path, LegacySchema::Dev2019);
    LegacyProfile {
        name: "Default",
        brightness: (7, 6),
        clock_1_local: 1,
        offsets: (-240, 480),
        use_24h: (1, 1, 0),
        date_format: "0",
        can_be_removed: 0,
        leds: Some(["1", "1", "2", "3", "0", "1", "2", "3", "1", "0", "0"]),
    }
    .insert(&connection, LegacyTable::X52Pro);
    LegacyProfile {
        name: "Off",
        brightness: (0, 0),
        date_format: "1",
        ..defaults("Off", 1)
    }
    .insert(&connection, LegacyTable::X52Pro);
    LegacyProfile {
        name: "Default",
        clock_1_local: 0,
        offsets: (345, -240),
        date_format: "2",
        leds: Some(["2"; 11]),
        ..defaults("Default", 0)
    }
    .insert(&connection, LegacyTable::X52);
    insert(
        &connection,
        "setting",
        &[("key", text("settings_launch_on_login")), ("value", int(1))],
    );
    insert(
        &connection,
        "setting",
        &[
            ("key", text("settings_show_app_indicator")),
            ("value", int(0)),
        ],
    );
    insert(
        &connection,
        "setting",
        &[
            ("key", text("settings_app_indicator_show_gpu_temp")),
            ("value", int(1)),
        ],
    );
}

/// The source-install database of S8 (released schema): both types' `Default Profile` at full
/// brightness, clocks at `-240` and `480`, one extra X52 Pro profile at the worked example `480`.
pub(crate) fn source_database(path: &Path) {
    let connection = create_database(path, LegacySchema::Release);
    defaults("Default Profile", 0).insert(&connection, LegacyTable::X52Pro);
    LegacyProfile {
        offsets: (480, 345),
        ..defaults("Asia", 1)
    }
    .insert(&connection, LegacyTable::X52Pro);
    LegacyProfile {
        leds: None,
        ..defaults("Default Profile", 0)
    }
    .insert(&connection, LegacyTable::X52);
}

fn rows(database: &LegacyDatabase, table: LegacyTable) -> Vec<&LegacyRow> {
    database
        .rows
        .iter()
        .filter(|row| row.table == table)
        .collect()
}

fn database(build: fn(&Path)) -> (tempfile::TempDir, LegacyDatabase) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    build(&path);
    let database = read_database(&path).unwrap();
    (dir, database)
}

#[test]
fn both_schemas_are_read_by_column_name() {
    for build in [flatpak_database as fn(&Path), source_database] {
        let (_dir, database) = database(build);
        assert_eq!(rows(&database, LegacyTable::X52Pro).len(), 2);
        assert_eq!(rows(&database, LegacyTable::X52).len(), 1);
        assert!(
            database.rows.iter().all(|row| row.problems.is_empty()),
            "{database:?}"
        );
    }
}

#[test]
fn the_values_are_converted_as_profiles_md_says() {
    let (_dir, database) = database(flatpak_database);
    let pro = &rows(&database, LegacyTable::X52Pro)[0].profile;
    assert_eq!(pro.led_brightness.level(), 28, "7 × 4");
    assert_eq!(pro.mfd_brightness.level(), 24, "6 × 4");
    assert!(pro.leds.is_on(SingleColourLed::Fire));
    assert!(!pro.leds.is_on(SingleColourLed::Throttle));
    assert_eq!(pro.leds.colour(BiColourLed::ButtonA), LedColour::Green);
    assert_eq!(pro.leds.colour(BiColourLed::ButtonB), LedColour::Red);
    assert_eq!(pro.leds.colour(BiColourLed::ButtonD), LedColour::Amber);
    assert_eq!(pro.leds.colour(BiColourLed::ButtonE), LedColour::Off);
    assert_eq!(
        pro.clock2.source,
        ZoneSpec::Fixed(FixedOffset::new(-240).unwrap())
    );
    assert!(!pro.clock3.h24);
    assert_eq!(pro.date_format, DateFormat::YyMmDd);
    assert!(!pro.shift);
    assert!(!pro.blink);
    assert_eq!(pro.mfd_lines, [String::new(), String::new(), String::new()]);

    let off = &rows(&database, LegacyTable::X52Pro)[1].profile;
    assert_eq!(off.led_brightness.level(), 0);
    assert_eq!(off.date_format, DateFormat::DdMmYy);

    let x52 = &rows(&database, LegacyTable::X52)[0].profile;
    assert_eq!(x52.led_brightness.level(), 128, "32 × 4");
    assert_eq!(x52.clock1.source, Clock1Source::Utc);
    assert_eq!(
        x52.clock2.source,
        ZoneSpec::Fixed(FixedOffset::new(345).unwrap())
    );
    assert_eq!(
        x52.clock3.source,
        ZoneSpec::Fixed(FixedOffset::new(-240).unwrap())
    );
    assert_eq!(x52.date_format, DateFormat::MmDdYy);
}

#[test]
fn x52_rows_never_take_the_led_columns() {
    let (_dir, database) = database(flatpak_database);
    let x52 = &rows(&database, LegacyTable::X52)[0].profile;
    assert_eq!(x52.leds, Profile::builtin_default(DeviceType::X52).leds);
}

#[test]
fn the_settings_are_read() {
    let (_dir, flatpak) = database(flatpak_database);
    assert_eq!(
        flatpak.settings,
        LegacySettings {
            launch_on_login: Some(true),
            show_app_indicator: Some(false),
        }
    );
    let (_dir, source) = database(source_database);
    assert_eq!(source.settings, LegacySettings::default());
}

#[test]
fn bad_values_get_the_type_default_and_the_row_is_imported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    let connection = create_database(&path, LegacySchema::Release);
    LegacyProfile {
        brightness: (33, -1),
        clock_1_local: 2,
        offsets: (900, -721),
        use_24h: (1, 5, 1),
        date_format: "7",
        leds: Some(["2", "4", "1", "1", "1", "1", "1", "1", "1", "1", "x"]),
        ..defaults("  ", 1)
    }
    .insert(&connection, LegacyTable::X52Pro);
    let long = "x".repeat(80);
    defaults(&long, 1).insert(&connection, LegacyTable::X52Pro);
    drop(connection);

    let database = read_database(&path).unwrap();
    let builtin = Profile::builtin(DeviceType::X52Pro, String::new());
    let bad = &database.rows[0];
    let keys: Vec<&str> = bad
        .problems
        .iter()
        .map(|problem| problem.key.as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "name",
            "led_brightness",
            "mfd_brightness",
            "clock_1_use_local_time",
            "clock_2_offset",
            "clock_2_use_24h",
            "clock_3_offset",
            "date_format",
            "led_fire",
            "led_throttle",
            "led_a",
        ]
    );
    assert_eq!(bad.profile.name, "Imported profile");
    assert_eq!(bad.profile.led_brightness, builtin.led_brightness);
    assert_eq!(bad.profile.clock1.source, builtin.clock1.source);
    assert_eq!(bad.profile.clock2, builtin.clock2);
    assert_eq!(bad.profile.clock3.source, builtin.clock3.source);
    assert_eq!(bad.profile.date_format, builtin.date_format);
    assert_eq!(
        bad.profile.leds.colour(BiColourLed::ButtonA),
        LedColour::Green
    );

    let long_row = &database.rows[1];
    assert_eq!(long_row.profile.name, "x".repeat(64));
    assert_eq!(long_row.problems.len(), 1);
}

#[test]
fn the_lowest_default_row_is_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    let connection = create_database(&path, LegacySchema::Release);
    for name in ["First", "Second"] {
        defaults(name, 0).insert(&connection, LegacyTable::X52Pro);
    }
    drop(connection);
    let database = read_database(&path).unwrap();
    let defaults: Vec<bool> = database.rows.iter().map(|row| row.is_default).collect();
    assert_eq!(defaults, [true, false]);
}

#[test]
fn databases_that_cannot_be_read_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.db");
    assert!(matches!(read_database(&missing), Err(DatabaseError::Io(_))));

    let garbage = dir.path().join("garbage.db");
    fs::write(&garbage, "not a database").unwrap();
    assert!(matches!(
        read_database(&garbage),
        Err(DatabaseError::Sqlite(_))
    ));

    let empty = dir.path().join("empty.db");
    Connection::open(&empty)
        .unwrap()
        .execute_batch("CREATE TABLE other (id INTEGER)")
        .unwrap();
    assert!(matches!(
        read_database(&empty),
        Err(DatabaseError::NoProfileTable)
    ));

    let huge = dir.path().join("huge.db");
    File::create(&huge)
        .unwrap()
        .set_len(DATABASE_LIMIT + 1)
        .unwrap();
    assert!(matches!(read_database(&huge), Err(DatabaseError::TooLarge)));
}

#[test]
fn the_database_is_never_modified() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    flatpak_database(&path);
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    read_database(&path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["gx52.db"], "no journal or other file left");
}

/// Legacy row → profile → TOML → profile gives the same profile, and the TOML is the stored
/// expected file (docs/profiles.md, "Clocks", "Round-trip tests"). `GX52_BLESS=1` rewrites the
/// expected files from the current output, to be reviewed before committing.
#[test]
fn legacy_rows_round_trip_through_toml() {
    for (database_name, build) in [
        ("flatpak", flatpak_database as fn(&Path)),
        ("source", source_database),
    ] {
        let (_dir, database) = database(build);
        for row in &database.rows {
            let written = schema::write(&row.profile);
            let expected_name = format!(
                "legacy/expected/{database_name}-{}-{}.toml",
                row.table.name(),
                row.row_id
            );
            if std::env::var_os("GX52_BLESS").is_some() {
                let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/testdata")
                    .join(&expected_name);
                fs::write(path, &written).unwrap();
            }
            let expected = crate::profile::tests::testdata(&expected_name);
            assert_eq!(
                written,
                String::from_utf8(expected).unwrap(),
                "{expected_name}"
            );
            match schema::parse(written.as_bytes(), Schema::CURRENT) {
                Ok(Parsed::Readable {
                    profile,
                    problems,
                    migrated_from: None,
                }) => {
                    assert_eq!(*profile, row.profile, "{expected_name}");
                    assert_eq!(problems, [], "{expected_name}");
                }
                other => panic!("{expected_name}: {other:?}"),
            }
        }
    }
}

fn empty_store(dir: &Path) -> ProfileStore {
    ProfileStore::load(dir, DefaultPersistence::MemoryOnly).0
}

#[test]
fn a_default_row_becomes_the_default_profile_when_its_file_is_missing() {
    let (_db, database) = database(source_database);
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(&database, &empty_store(dir.path()));
    let targets: Vec<String> = plan
        .entries
        .iter()
        .map(|entry| entry.target.to_string())
        .collect();
    assert_eq!(targets[0], "x52-pro-default");
    assert_eq!(targets[2], "x52-default");
    assert_eq!(
        plan.entries[0].name, "Default",
        "the default keeps its name"
    );
    assert_eq!(plan.entries[1].name, "Asia");
    assert_eq!(ProfileId::parse(&targets[1]).unwrap().default_type(), None);
}

#[test]
fn a_default_row_becomes_a_normal_profile_when_the_default_file_exists() {
    let (_db, database) = database(source_database);
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = ProfileStore::load(dir.path(), DefaultPersistence::Write);
    let plan = plan(&database, &store);
    assert_eq!(plan.entries[0].target.default_type(), None);
    assert_eq!(plan.entries[0].name, "Default Profile");
}

#[test]
fn imported_names_are_made_unique_within_the_type() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    let connection = create_database(&path, LegacySchema::Release);
    let long = "y".repeat(64);
    for name in ["Default", "Combat", "Combat", long.as_str(), long.as_str()] {
        defaults(name, 1).insert(&connection, LegacyTable::X52Pro);
    }
    LegacyProfile {
        leds: None,
        ..defaults("Combat", 1)
    }
    .insert(&connection, LegacyTable::X52);
    drop(connection);
    let database = read_database(&path).unwrap();
    let profiles = tempfile::tempdir().unwrap();
    let plan = plan(&database, &empty_store(profiles.path()));
    let names: Vec<&str> = plan
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    let second_long = format!("{} (2)", "y".repeat(60));
    assert_eq!(
        names,
        [
            "Default (2)",
            "Combat",
            "Combat (2)",
            long.as_str(),
            second_long.as_str(),
            "Combat"
        ]
    );
}

#[test]
fn the_plan_file_round_trips() {
    let (_db, database) = database(flatpak_database);
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(&database, &empty_store(dir.path()));
    assert_eq!(Plan::from_toml(&plan.to_toml()), Some(plan.clone()));
    let completed = Plan {
        completed: true,
        ..plan
    };
    assert_eq!(Plan::from_toml(&completed.to_toml()), Some(completed));
    assert_eq!(Plan::from_toml("version = 2"), None);
    assert_eq!(Plan::from_toml("not toml ["), None);
}

/// Imports the maintainer's real databases (`GX52_LEGACY_DB`) into `GX52_LEGACY_OUT` for a check
/// by eye; nothing from them is committed. Run with
/// `GX52_LEGACY_DB=... GX52_LEGACY_OUT=... cargo test -p gx52 real_database -- --ignored`.
#[test]
#[ignore = "needs a real GX52 0.7 database"]
fn real_database() {
    let database_path = PathBuf::from(std::env::var_os("GX52_LEGACY_DB").unwrap());
    let out = PathBuf::from(std::env::var_os("GX52_LEGACY_OUT").unwrap());
    let startup =
        crate::profile::startup::prepare_profiles(&out, &database_path, false, &mut FileWriter);
    let mut summary = format!("{:#?}\n", startup.import);
    for (id, entry) in startup.store.entries() {
        let _ = writeln!(summary, "{id}: {entry:?}");
    }
    fs::write(out.join("summary.txt"), summary).unwrap();
}

#[test]
fn text_that_is_not_utf8_costs_only_its_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gx52.db");
    let connection = create_database(&path, LegacySchema::Release);
    defaults("Broken", 1).insert(&connection, LegacyTable::X52Pro);
    connection
        .execute(
            "UPDATE x52_pro_profile SET name = CAST(X'4E6967687420FF' AS TEXT), led_a = CAST(X'FF' AS TEXT)",
            [],
        )
        .unwrap();
    drop(connection);
    let database = read_database(&path).unwrap();
    let row = &database.rows[0];
    assert_eq!(row.profile.name, "Night \u{fffd}");
    assert_eq!(row.problems.len(), 1, "{:?}", row.problems);
    assert_eq!(row.problems[0].key, "led_a");
}

/// GX52 0.7 stopped in the middle of a write: the database comes with a hot rollback journal,
/// which a read-only open cannot roll back. The import reads the rolled-back copy and leaves both
/// original files untouched.
#[test]
fn a_database_with_a_hot_journal_is_read_as_before_the_interrupted_write() {
    let source = tempfile::tempdir().unwrap();
    let source_path = source.path().join("gx52.db");
    source_database(&source_path);
    let crashed = tempfile::tempdir().unwrap();
    let path = crashed.path().join("gx52.db");
    {
        let connection = Connection::open(&source_path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode = DELETE; PRAGMA cache_size = 1; BEGIN;
                 UPDATE x52_pro_profile SET name = 'Half written', led_brightness = 99;
                 CREATE TABLE filler (data BLOB);
                 INSERT INTO filler VALUES (zeroblob(200000));",
            )
            .unwrap();
        // A copy taken now is what a crash leaves behind: the changed pages and a hot journal.
        fs::copy(&source_path, &path).unwrap();
        fs::copy(
            source.path().join("gx52.db-journal"),
            crashed.path().join("gx52.db-journal"),
        )
        .unwrap();
    }
    let read_only = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert!(
        read_only
            .query_row("SELECT count(*) FROM x52_pro_profile", [], |row| row
                .get::<_, i64>(0))
            .is_err(),
        "the fixture needs a hot journal that a read-only open cannot roll back"
    );
    drop(read_only);
    let before: Vec<Vec<u8>> = ["gx52.db", "gx52.db-journal"]
        .iter()
        .map(|name| fs::read(crashed.path().join(name)).unwrap())
        .collect();

    let database = read_database(&path).unwrap();
    let names: Vec<&str> = database
        .rows
        .iter()
        .map(|row| row.profile.name.as_str())
        .collect();
    assert_eq!(names, ["Default Profile", "Asia", "Default Profile"]);
    assert!(database.rows.iter().all(|row| row.problems.is_empty()));
    let after: Vec<Vec<u8>> = ["gx52.db", "gx52.db-journal"]
        .iter()
        .map(|name| fs::read(crashed.path().join(name)).unwrap())
        .collect();
    assert_eq!(after, before, "the originals are never touched");
}

/// A reader that returns, for each read of the set (database, then each companion), the state
/// `states` gives for that read's index, and the last state from then on.
fn scripted(states: Vec<Snapshot>) -> impl FnMut(&Path) -> Result<Option<Vec<u8>>, DatabaseError> {
    let mut calls = 0;
    move |path| {
        let state = &states[(calls / 3).min(states.len() - 1)];
        let file = calls % 3;
        calls += 1;
        let name = path.file_name().unwrap().to_str().unwrap();
        let expected = ["gx52.db", "gx52.db-journal", "gx52.db-wal"][file];
        assert_eq!(name, expected);
        Ok(match file {
            0 => Some(state.database.clone()),
            other => state.companions[other - 1].clone(),
        })
    }
}

fn state(database: &str, journal: Option<&str>) -> Snapshot {
    Snapshot {
        database: database.as_bytes().to_vec(),
        companions: [journal.map(|journal| journal.as_bytes().to_vec()), None],
    }
}

#[test]
fn a_rollback_between_two_files_is_never_taken_as_a_snapshot() {
    let path = Path::new("/nowhere/gx52.db");
    // First read: uncommitted pages with their journal. The writer then rolls back: the pages are
    // restored and the journal deleted.
    let mut read = scripted(vec![
        state("uncommitted", Some("journal")),
        state("committed", None),
    ]);
    let taken = snapshot(path, Duration::ZERO, &mut read).unwrap();
    assert_eq!(taken, state("committed", None));

    // The mixed state the race used to produce, uncommitted pages without their journal, is never
    // returned unless two reads agree on it.
    let mut read = scripted(vec![
        state("uncommitted", None),
        state("committed", None),
        state("committed", None),
    ]);
    assert_eq!(
        snapshot(path, Duration::ZERO, &mut read).unwrap(),
        state("committed", None)
    );
}

#[test]
fn a_database_that_keeps_changing_defers_the_import() {
    let path = Path::new("/nowhere/gx52.db");
    let mut count = 0_u32;
    let mut read = |path: &Path| {
        count += 1;
        Ok(path
            .ends_with("gx52.db")
            .then(|| count.to_string().into_bytes()))
    };
    let error = snapshot(path, Duration::ZERO, &mut read).unwrap_err();
    assert!(matches!(error, DatabaseError::Changing));
    assert!(!error.is_final());
}
