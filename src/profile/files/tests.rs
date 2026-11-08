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

use std::fs;

use super::*;

#[test]
fn reads_up_to_the_limit_and_refuses_more() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.toml");
    fs::write(&path, vec![b'#'; 10]).unwrap();
    assert_eq!(read_limited(&path, 10).unwrap().len(), 10);
    assert!(matches!(
        read_limited(&path, 9),
        Err(ReadError::TooLarge { limit: 9 })
    ));
}

#[test]
fn replace_writes_over_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.toml");
    fs::write(&path, "old").unwrap();
    replace(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no temporary file left"
    );
}

#[test]
fn create_new_never_replaces_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.toml");
    assert_eq!(create_new(&path, b"first").unwrap(), Created::Written);
    assert_eq!(
        create_new(&path, b"second").unwrap(),
        Created::AlreadyExists
    );
    assert_eq!(fs::read(&path).unwrap(), b"first");
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no temporary file left"
    );
}

#[test]
fn backups_never_overwrite_backups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x52-default.toml");
    let now: Timestamp = "2026-11-08T09:10:11Z".parse().unwrap();
    let suffix = rejected_suffix(now);
    assert_eq!(suffix, "rejected-20261108T091011Z");

    fs::write(&path, "one").unwrap();
    let first = backup(&path, &suffix).unwrap();
    fs::write(&path, "two").unwrap();
    let second = backup(&path, &suffix).unwrap();
    fs::write(&path, "three").unwrap();
    let third = backup(&path, &suffix).unwrap();

    let name = |path: &Path| path.file_name().unwrap().to_str().unwrap().to_owned();
    assert_eq!(name(&first), "x52-default.toml.rejected-20261108T091011Z");
    assert_eq!(
        name(&second),
        "x52-default.toml.rejected-20261108T091011Z-2"
    );
    assert_eq!(name(&third), "x52-default.toml.rejected-20261108T091011Z-3");
    assert_eq!(fs::read(first).unwrap(), b"one");
    assert_eq!(fs::read(second).unwrap(), b"two");
    assert_eq!(fs::read(third).unwrap(), b"three");
}

#[test]
fn schema_backups_are_named_after_the_old_version() {
    assert_eq!(schema_suffix(1), "schema-1.bak");
}
