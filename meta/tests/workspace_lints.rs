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
// Adapted from swarm-device-access/meta/tests/workspace_lints.rs at 4cd31c4, originally
// Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi

//! The shared lint set is declared once, in the root `[workspace.lints]`, and a package only obeys
//! it when its own manifest opts in: `[lints]` is not inherited by workspace members. A member that
//! forgets `[lints] workspace = true` builds and passes clippy with the default lints, silently.
//!
//! The checks are functions of the manifests, so each one is also run on manifests built to break
//! it: the negative controls.

use meta::repo;
use toml::Table;

fn parse(text: &str, origin: &str) -> Table {
    text.parse::<Table>()
        .unwrap_or_else(|error| panic!("parse {origin}: {error}"))
}

fn read_manifest(relative: &str) -> Table {
    parse(&repo::read(relative), relative)
}

/// What is wrong with the root manifest's `[workspace.lints]`, if anything.
fn workspace_lints_problems(root: &Table) -> Vec<String> {
    let Some(lints) = root
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .and_then(toml::Value::as_table)
    else {
        return vec!["the root Cargo.toml has no [workspace.lints] table".to_owned()];
    };
    ["rust", "clippy"]
        .into_iter()
        .filter(|section| !lints.contains_key(*section))
        .map(|section| format!("[workspace.lints.{section}] is missing"))
        .collect()
}

/// Every workspace member whose manifest does not opt into the workspace lints, and every member
/// listed as a glob (which this check could not follow), as messages. `read_member` reads a
/// member's manifest from its path.
fn members_without_workspace_lints(
    root: &Table,
    read_member: impl Fn(&str) -> Table,
) -> Vec<String> {
    let Some(members) = root
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
    else {
        return vec!["the root Cargo.toml lists no [workspace] members".to_owned()];
    };
    if members.is_empty() {
        return vec!["the workspace lists no members".to_owned()];
    }
    let mut problems = Vec::new();
    for member in members {
        let Some(member) = member.as_str() else {
            problems.push(format!("member {member} is not a path string"));
            continue;
        };
        if member.contains('*') {
            problems.push(format!(
                "member {member:?} is a glob; list members explicitly so this check sees each one"
            ));
            continue;
        }
        let opts_in = read_member(member)
            .get("lints")
            .and_then(|lints| lints.get("workspace"))
            .and_then(toml::Value::as_bool);
        if opts_in != Some(true) {
            problems.push(format!(
                "{member}/Cargo.toml does not declare [lints] workspace = true"
            ));
        }
    }
    problems
}

#[test]
fn root_manifest_declares_workspace_lints() {
    let problems = workspace_lints_problems(&read_manifest("Cargo.toml"));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_workspace_member_opts_into_the_workspace_lints() {
    let problems = members_without_workspace_lints(&read_manifest("Cargo.toml"), |member| {
        read_manifest(&format!("{member}/Cargo.toml"))
    });
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Negative control: a root without the lint tables is refused.
#[test]
fn a_root_without_workspace_lints_is_refused() {
    let missing = parse("[workspace]\nmembers = [\".\"]\n", "fixture");
    assert_eq!(
        workspace_lints_problems(&missing),
        ["the root Cargo.toml has no [workspace.lints] table"]
    );
    let clippy_only = parse("[workspace.lints.clippy]\npedantic = \"warn\"\n", "fixture");
    assert_eq!(
        workspace_lints_problems(&clippy_only),
        ["[workspace.lints.rust] is missing"]
    );
}

/// Negative control: a member that does not opt in, one that opts out, and a glob are each
/// refused, while the member that opts in passes.
#[test]
fn a_member_without_the_opt_in_is_refused() {
    let root = parse(
        "[workspace]\nmembers = [\".\", \"silent\", \"opted-out\", \"crates/*\"]\n",
        "fixture",
    );
    let problems = members_without_workspace_lints(&root, |member| match member {
        "." => parse("[lints]\nworkspace = true\n", member),
        "opted-out" => parse("[lints]\nworkspace = false\n", member),
        _ => parse("[package]\nname = \"silent\"\n", member),
    });
    assert_eq!(
        problems,
        [
            "silent/Cargo.toml does not declare [lints] workspace = true",
            "opted-out/Cargo.toml does not declare [lints] workspace = true",
            "member \"crates/*\" is a glob; list members explicitly so this check sees each one",
        ]
    );
}
