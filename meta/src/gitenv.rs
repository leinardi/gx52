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
// Adapted from swarm-device-access/meta/src/gitenv.rs at 4cd31c4, originally Apache-2.0-licensed,
// Copyright (c) 2026 Roberto Leinardi

//! An environment for git commands that cannot reach the repository being committed.
//!
//! The meta-tests also run inside a pre-commit hook, and git exports its own location to hooks:
//! `GIT_INDEX_FILE` always, `GIT_DIR` and others in some setups, absolute in a linked worktree. A
//! git command run in a scratch repository with that environment acts on the real one instead -- a
//! `git add` there writes into the index of the commit being made. [`isolate`] strips those
//! variables and the user's own configuration, so a scratch repository is only ever itself.

use std::env;
use std::process::Command;

/// The variables that point git at a repository other than the working directory's.
const REPOSITORY_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
];

/// Removes from `command` the variables that redirect git and the user's global and system
/// configuration.
pub fn isolate(command: &mut Command) -> &mut Command {
    for (name, _) in env::vars_os() {
        let redirects = name
            .to_str()
            .is_some_and(|name| REPOSITORY_VARS.contains(&name) || name.starts_with("GIT_CONFIG"));
        if redirects {
            command.env_remove(&name);
        }
    }
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
}
