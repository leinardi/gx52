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

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use tempfile::TempDir;
use x52::input::Mode;

use super::*;
use crate::profile::files::Created;
use crate::profile::legacy::FileWriter;
use crate::profile::legacy::tests::{flatpak_database, source_database};
use crate::profile::store::Origin;
use crate::profile::{ProfileId, legacy};

/// A writer that fails its `fail_at`-th call (from 0), either before writing or, with
/// `after_write`, after the file is in place, as a crash between the write and its report would.
struct FailingWriter {
    calls: usize,
    fail_at: usize,
    after_write: bool,
}

impl FailingWriter {
    fn fail(&mut self) -> bool {
        let fail = self.calls == self.fail_at;
        self.calls += 1;
        fail
    }
}

fn injected() -> io::Error {
    io::Error::other("injected failure")
}

impl ProfileWriter for FailingWriter {
    fn create_new(&mut self, path: &Path, contents: &[u8]) -> io::Result<Created> {
        if !self.fail() {
            return FileWriter.create_new(path, contents);
        }
        if self.after_write {
            FileWriter.create_new(path, contents)?;
        }
        Err(injected())
    }

    fn replace(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
        if !self.fail() {
            return FileWriter.replace(path, contents);
        }
        if self.after_write {
            FileWriter.replace(path, contents)?;
        }
        Err(injected())
    }
}

/// Counts the writes of a run.
#[derive(Default)]
struct CountingWriter(usize);

impl ProfileWriter for CountingWriter {
    fn create_new(&mut self, path: &Path, contents: &[u8]) -> io::Result<Created> {
        self.0 += 1;
        FileWriter.create_new(path, contents)
    }

    fn replace(&mut self, path: &Path, contents: &[u8]) -> io::Result<()> {
        self.0 += 1;
        FileWriter.replace(path, contents)
    }
}

/// The profile files of a directory: the defaults by name, the others by contents only.
type Profiles = (BTreeMap<String, Vec<u8>>, Vec<Vec<u8>>);

/// A config directory and a legacy database built by `build`.
struct Setup {
    config: TempDir,
    database: PathBuf,
    _database_dir: TempDir,
}

fn setup(build: fn(&Path)) -> Setup {
    let database_dir = tempfile::tempdir().unwrap();
    let database = database_dir.path().join("gx52.db");
    build(&database);
    Setup {
        config: tempfile::tempdir().unwrap(),
        database,
        _database_dir: database_dir,
    }
}

impl Setup {
    /// One start, with the flag as the main thread would keep it: set when the import is done,
    /// then the plan marked completed.
    fn start(&self, done: &mut bool, writer: &mut impl ProfileWriter) -> Startup {
        let startup = self.start_and_exit(*done, writer);
        *done |= startup.import.is_done();
        if let ImportOutcome::Completed { plan, .. } = &startup.import {
            finish_import(self.config.path(), plan, writer);
        }
        startup
    }

    /// One start that ends before the main thread applies the settings and sets the flag.
    fn start_and_exit(&self, done: bool, writer: &mut impl ProfileWriter) -> Startup {
        prepare_profiles(self.config.path(), &self.database, done, writer)
    }

    /// The profile files: the defaults by name, the others by contents only, since their ids are
    /// random.
    fn profiles(&self) -> Profiles {
        let mut defaults = BTreeMap::new();
        let mut others = Vec::new();
        for entry in fs::read_dir(profiles_dir(self.config.path())).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let contents = fs::read(entry.path()).unwrap();
            if name.ends_with("-default.toml") {
                defaults.insert(name, contents);
            } else {
                others.push(contents);
            }
        }
        others.sort();
        (defaults, others)
    }
}

/// The profiles after one clean start and the next one, and the number of writes the first start
/// made.
fn clean_run(build: fn(&Path)) -> (Profiles, usize) {
    let setup = setup(build);
    let mut done = false;
    let mut writer = CountingWriter::default();
    let first = setup.start(&mut done, &mut writer);
    assert!(matches!(first.import, ImportOutcome::Completed { .. }));
    let write_count = writer.0;
    setup.start(&mut done, &mut FileWriter);
    (setup.profiles(), write_count)
}

#[test]
fn a_clean_start_imports_every_row_and_the_settings() {
    let setup = setup(flatpak_database);
    let mut done = false;
    let startup = setup.start(&mut done, &mut FileWriter);
    let ImportOutcome::Completed {
        report, settings, ..
    } = &startup.import
    else {
        panic!("{:?}", startup.import);
    };
    assert!(done);
    assert_eq!(report.imported.len(), 3);
    assert!(report.conflicts.is_empty());
    assert_eq!(settings.launch_on_login, Some(true));
    let pro_default = startup
        .store
        .loaded(&ProfileId::default_for(x52::device::DeviceType::X52Pro))
        .unwrap();
    assert_eq!(pro_default.origin, Origin::File);
    assert_eq!(
        pro_default
            .profile
            .settings_for(Mode::One)
            .led_brightness
            .level(),
        28,
        "the legacy values"
    );
    assert_eq!(pro_default.profile.name, "Default");
    assert_eq!(startup.store.entries().count(), 3);
}

#[test]
fn an_interrupted_import_resumes_to_the_result_of_a_clean_one() {
    for build in [flatpak_database as fn(&Path), source_database] {
        let (expected, write_count) = clean_run(build);
        for after_write in [false, true] {
            // The last write marks the plan completed; its failure is covered below.
            for fail_at in 0..write_count - 1 {
                let case = format!("fail_at {fail_at}, after_write {after_write}");
                let setup = setup(build);
                let mut done = false;
                let mut writer = FailingWriter {
                    calls: 0,
                    fail_at,
                    after_write,
                };
                let failed = setup.start(&mut done, &mut writer);
                assert!(
                    matches!(failed.import, ImportOutcome::WriteFailed(_)),
                    "{case}"
                );
                assert!(!done, "{case}");
                let resumed = setup.start(&mut done, &mut FileWriter);
                assert!(
                    matches!(resumed.import, ImportOutcome::Completed { ref report, .. } if report.conflicts.is_empty()),
                    "{case}: {:?}",
                    resumed.import
                );
                assert!(done, "{case}");
                setup.start(&mut done, &mut FileWriter);
                assert_eq!(setup.profiles(), expected, "{case}");
            }
        }
    }
}

#[test]
fn the_imported_defaults_are_not_replaced_by_the_builtin_ones_after_a_failure() {
    let setup = setup(flatpak_database);
    let mut done = false;
    // Call 0 writes the plan, call 1 the X52 Pro default.
    let mut writer = FailingWriter {
        calls: 0,
        fail_at: 1,
        after_write: false,
    };
    setup.start(&mut done, &mut writer);
    let pro_default = profiles_dir(setup.config.path()).join("x52-pro-default.toml");
    assert!(
        !pro_default.exists(),
        "no built-in default written while pending"
    );
    setup.start(&mut done, &mut FileWriter);
    let written = fs::read_to_string(pro_default).unwrap();
    assert!(written.contains("led_brightness = 28"), "{written}");
}

#[test]
fn unrelated_files_never_stand_in_for_imported_rows() {
    let (expected, _) = clean_run(source_database);
    let setup = setup(source_database);
    let dir = profiles_dir(setup.config.path());
    fs::create_dir_all(&dir).unwrap();
    let unrelated = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/profiles/valid-x52-pro.toml"),
    )
    .unwrap();
    for name in [
        "legacy-x52-pro-2.toml",
        "legacy-x52-pro-1.toml",
        "notes.txt",
    ] {
        fs::write(dir.join(name), &unrelated).unwrap();
    }
    let mut done = false;
    // Interrupted after the plan, then resumed: every row is still imported.
    let mut writer = FailingWriter {
        calls: 0,
        fail_at: 1,
        after_write: false,
    };
    setup.start(&mut done, &mut writer);
    let resumed = setup.start(&mut done, &mut FileWriter);
    let ImportOutcome::Completed { report, .. } = &resumed.import else {
        panic!("{:?}", resumed.import);
    };
    assert_eq!(report.imported.len(), 3);
    assert!(report.conflicts.is_empty());
    let (defaults, mut others) = setup.profiles();
    others.retain(|contents| *contents != unrelated);
    assert_eq!((defaults, others), expected);
    assert_eq!(
        fs::read(dir.join("legacy-x52-pro-2.toml")).unwrap(),
        unrelated
    );
}

#[test]
fn a_target_changed_between_attempts_is_kept_and_reported() {
    let setup = setup(source_database);
    let mut done = false;
    let mut writer = FailingWriter {
        calls: 0,
        fail_at: 2,
        after_write: false,
    };
    setup.start(&mut done, &mut writer);
    // The X52 Pro default was written; someone edits it before the next start.
    let pro_default = profiles_dir(setup.config.path()).join("x52-pro-default.toml");
    let edited = fs::read_to_string(&pro_default)
        .unwrap()
        .replace("led_brightness = 128", "led_brightness = 5");
    fs::write(&pro_default, &edited).unwrap();
    let resumed = setup.start(&mut done, &mut FileWriter);
    let ImportOutcome::Completed { report, .. } = &resumed.import else {
        panic!("{:?}", resumed.import);
    };
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].target.as_str(), "x52-pro-default");
    assert_eq!(fs::read_to_string(&pro_default).unwrap(), edited);
}

#[test]
fn an_unreadable_database_ends_the_import_without_writing() {
    let setup = setup(|path| fs::write(path, "not a database").unwrap());
    let mut done = false;
    let startup = setup.start(&mut done, &mut CountingWriter::default());
    assert!(matches!(startup.import, ImportOutcome::DatabaseFailed(_)));
    assert!(done);
    assert!(!legacy::plan_path(setup.config.path()).exists());
    // The import is over, so the built-in defaults are written by this very load.
    let (defaults, others) = setup.profiles();
    assert_eq!(defaults.len(), 2);
    assert!(others.is_empty());
}

#[test]
fn nothing_runs_again_after_a_completed_import() {
    let setup = setup(source_database);
    let mut done = false;
    setup.start(&mut done, &mut FileWriter);
    let before = setup.profiles();
    let mut writer = CountingWriter::default();
    let again = setup.start(&mut done, &mut writer);
    assert!(matches!(again.import, ImportOutcome::NotDue));
    assert_eq!(writer.0, 0);

    // Even with the flag lost, the completed plan keeps the import from running again.
    let mut lost = false;
    let mut writer = CountingWriter::default();
    let rerun = setup.start(&mut lost, &mut writer);
    assert!(matches!(rerun.import, ImportOutcome::NotDue));
    assert_eq!(writer.0, 0);
    assert_eq!(setup.profiles(), before);
}

#[test]
fn a_failed_completion_mark_still_completes_and_a_rerun_writes_nothing_new() {
    let (expected, write_count) = clean_run(source_database);
    let setup = setup(source_database);
    let mut done = false;
    let mut writer = FailingWriter {
        calls: 0,
        fail_at: write_count - 1,
        after_write: false,
    };
    let first = setup.start(&mut done, &mut writer);
    assert!(matches!(first.import, ImportOutcome::Completed { .. }));
    assert!(done);
    // The flag is lost as well: the unmarked plan runs again and finds every file in place.
    let mut lost = false;
    let rerun = setup.start(&mut lost, &mut FileWriter);
    assert!(
        matches!(rerun.import, ImportOutcome::Completed { ref report, .. } if report.conflicts.is_empty())
    );
    assert_eq!(setup.profiles(), expected);
}

#[test]
fn an_exit_before_the_settings_are_written_imports_them_at_the_next_start() {
    let (expected, _) = clean_run(flatpak_database);
    let setup = setup(flatpak_database);
    let first = setup.start_and_exit(false, &mut FileWriter);
    assert!(matches!(first.import, ImportOutcome::Completed { .. }));
    assert!(
        !legacy::saved_plan(setup.config.path()).unwrap().completed,
        "the worker never marks the plan completed itself"
    );

    // The flag was never written: the next start runs the plan again, writes no new profile and
    // returns the settings to apply.
    let before = setup.profiles();
    let mut done = false;
    let next = setup.start(&mut done, &mut FileWriter);
    let ImportOutcome::Completed {
        report, settings, ..
    } = &next.import
    else {
        panic!("{:?}", next.import);
    };
    assert!(report.conflicts.is_empty());
    assert_eq!(settings.launch_on_login, Some(true));
    assert_eq!(setup.profiles(), before, "no profile written again");
    assert!(done);
    assert!(legacy::saved_plan(setup.config.path()).unwrap().completed);
    setup.start(&mut done, &mut FileWriter);
    assert_eq!(setup.profiles(), expected);
}

#[test]
fn a_deferred_import_is_not_done() {
    let outcome = ImportOutcome::Deferred(DatabaseError::Changing);
    assert!(!outcome.is_done());
    assert!(!DatabaseError::Changing.is_final());
}

#[test]
fn no_database_means_no_import() {
    let config = tempfile::tempdir().unwrap();
    let startup = prepare_profiles(
        config.path(),
        &config.path().join("missing.db"),
        false,
        &mut FileWriter,
    );
    assert!(matches!(startup.import, ImportOutcome::NotDue));
    assert!(
        !startup.import.is_done(),
        "a database copied in later is still imported"
    );
    assert_eq!(startup.load_report.defaults_written.len(), 2);
}
