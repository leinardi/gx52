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

//! Scratch git repositories for the tests that drive a repository script: files, commits and tags
//! in a temporary directory, with every git command and script run under [`gitenv::isolate`], so
//! under the commit hook nothing reaches the repository being committed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::{gitenv, repo};

/// A temporary git repository, removed when dropped.
#[derive(Debug)]
pub struct Scratch {
    directory: tempfile::TempDir,
}

/// What a script printed and how it exited.
#[derive(Debug)]
pub struct Run {
    /// Whether it exited with status 0.
    pub success: bool,
    /// Its standard output and standard error, in that order.
    pub text: String,
}

impl Scratch {
    /// An empty repository on branch `main`, with a committer identity of its own.
    ///
    /// # Panics
    ///
    /// The directory cannot be made, or git fails.
    #[must_use]
    pub fn new() -> Self {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("scratch directory: {error}"));
        let scratch = Self { directory };
        scratch.git(&["init", "--quiet", "--initial-branch=main"]);
        scratch.git(&["config", "user.name", "Scratch"]);
        scratch.git(&["config", "user.email", "scratch@example.com"]);
        scratch.git(&["config", "commit.gpgsign", "false"]);
        scratch.git(&["config", "tag.gpgsign", "false"]);
        scratch
    }

    /// The repository's directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    /// The absolute path of a slash-separated path inside the repository.
    #[must_use]
    pub fn join(&self, relative: &str) -> PathBuf {
        self.path().join(relative)
    }

    /// Writes `content` to `relative`, creating its directory.
    ///
    /// # Panics
    ///
    /// The file cannot be written.
    pub fn write(&self, relative: &str, content: &str) {
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create the directory of {relative}: {error}"));
        }
        fs::write(&path, content).unwrap_or_else(|error| panic!("write {relative}: {error}"));
    }

    /// Reads `relative`.
    ///
    /// # Panics
    ///
    /// The file cannot be read.
    #[must_use]
    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"))
    }

    /// Copies the repository's own `relative` (a script) to the same path here, executable.
    ///
    /// # Panics
    ///
    /// It cannot be read or written.
    pub fn copy_script(&self, relative: &str) {
        self.write(relative, &repo::read(relative));
        fs::set_permissions(self.join(relative), fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("make {relative} executable: {error}"));
    }

    /// Runs git with `args`.
    ///
    /// # Panics
    ///
    /// git cannot run or fails, with its standard error.
    pub fn git(&self, args: &[&str]) {
        let _stdout = self.git_stdout(args);
    }

    /// Runs git with `args` and returns its trimmed standard output.
    ///
    /// # Panics
    ///
    /// git cannot run or fails, with its standard error.
    #[must_use]
    pub fn git_stdout(&self, args: &[&str]) -> String {
        let output = gitenv::isolate(Command::new("git").args(args).current_dir(self.path()))
            .output()
            .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// Stages everything and commits it (an empty commit when nothing changed).
    pub fn commit(&self, message: &str) {
        self.git(&["add", "--all"]);
        self.git(&[
            "commit",
            "--quiet",
            "--allow-empty",
            "--no-verify",
            "-m",
            message,
        ]);
    }

    /// Tags the current commit.
    pub fn tag(&self, name: &str) {
        self.git(&["tag", name]);
    }

    /// `git status --porcelain`: empty when the tree is clean.
    #[must_use]
    pub fn status(&self) -> String {
        self.git_stdout(&["status", "--porcelain"])
    }

    /// Runs the script at `relative` with `args` and the extra `env`, from the repository root.
    ///
    /// # Panics
    ///
    /// The script cannot be started.
    #[must_use]
    pub fn run(&self, relative: &str, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(self.join(relative));
        command.args(args).current_dir(self.path());
        for (name, value) in env {
            command.env(name, value);
        }
        let output: Output = gitenv::isolate(&mut command)
            .output()
            .unwrap_or_else(|error| panic!("run {relative}: {error}"));
        Run {
            success: output.status.success(),
            text: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}
