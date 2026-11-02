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
//
// Adapted from swarm-device-access/meta/src/repo.rs at 4cd31c4, originally Apache-2.0-licensed,
// Copyright (c) 2026 Roberto Leinardi

//! The repository under test: its root, its files and its tracked paths.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root: the meta crate sits one level below it.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The absolute path of a slash-separated path relative to the root.
#[must_use]
pub fn path(relative: &str) -> PathBuf {
    root().join(relative)
}

/// Reads a file by its slash-separated path relative to the root.
///
/// # Panics
///
/// The file cannot be read, naming it.
#[must_use]
pub fn read(relative: &str) -> String {
    fs::read_to_string(path(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The tracked files matching `pathspecs`, as slash-separated paths relative to the root. Run
/// with the repository's own git environment: under the commit hook it names the index being
/// committed.
///
/// # Panics
///
/// git cannot list them.
#[must_use]
pub fn tracked(pathspecs: &[&str]) -> Vec<String> {
    ls_files(&[], pathspecs)
}

/// The files matching `pathspecs` that are tracked or new (untracked and not ignored), as
/// slash-separated paths relative to the root: what the next `git add -A` would commit, so a check
/// that must hold for every source file also sees one nobody has added yet.
///
/// # Panics
///
/// git cannot list them.
#[must_use]
pub fn tracked_or_new(pathspecs: &[&str]) -> Vec<String> {
    let mut files = ls_files(&["--cached", "--others", "--exclude-standard"], pathspecs);
    files.sort();
    files.dedup();
    files
}

fn ls_files(options: &[&str], pathspecs: &[&str]) -> Vec<String> {
    let output = Command::new("git")
        .arg("ls-files")
        .arg("-z")
        .args(options)
        .arg("--")
        .args(pathspecs)
        .current_dir(root())
        .output()
        .unwrap_or_else(|error| panic!("run git ls-files: {error}"));
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every file under `directory` (relative to the root, recursively) whose name ends with one of
/// `suffixes`, as slash-separated paths relative to the root, sorted.
///
/// # Panics
///
/// The directory cannot be read.
#[must_use]
pub fn files_under(directory: &str, suffixes: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_owned()];
    while let Some(current) = pending.pop() {
        let entries =
            fs::read_dir(path(&current)).unwrap_or_else(|error| panic!("list {current}: {error}"));
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| panic!("list {current}: {error}"));
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = format!("{current}/{name}");
            let kind = entry
                .file_type()
                .unwrap_or_else(|error| panic!("inspect {relative}: {error}"));
            if kind.is_dir() {
                pending.push(relative);
            } else if kind.is_file() && suffixes.iter().any(|suffix| name.ends_with(suffix)) {
                found.push(relative);
            }
        }
    }
    found.sort();
    found
}
