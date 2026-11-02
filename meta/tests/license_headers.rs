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
// Adapted from swarm-device-access/meta/tests/license_headers.rs at 4cd31c4, originally
// Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi

//! Drives scripts/check-license-headers.sh: on this repository, and on scratch repositories that
//! hold each kind of file it must reject or let through, grouped into several tests so they run in
//! parallel.
//!
//! Every case that must be refused is the negative control of one that must pass: a missing
//! header, another licence, another holder, a header past the window, an incomplete provenance
//! comment and a pasted MIT or Apache-2.0 block. The scratch repositories run git and the script
//! with an isolated git environment (`meta::gitenv`), so under the commit hook they never touch the
//! index being committed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use meta::{gitenv, repo};

const SCRIPT_PATH: &str = "scripts/check-license-headers.sh";

/// Scratch files under this prefix are written but never added: the check reads tracked files
/// only.
const UNTRACKED_PREFIX: &str = "untracked/";

/// The GPL-3.0-or-later notice below the copyright line, with `$prefix` as the comment marker.
macro_rules! gpl_body {
    ($prefix:literal) => {
        concat!(
            $prefix,
            "\n",
            $prefix,
            " This program is free software: you can redistribute it and/or modify\n",
            $prefix,
            " it under the terms of the GNU General Public License as published by\n",
            $prefix,
            " the Free Software Foundation, either version 3 of the License, or\n",
            $prefix,
            " (at your option) any later version.\n",
            $prefix,
            "\n",
            $prefix,
            " This program is distributed in the hope that it will be useful,\n",
            $prefix,
            " but WITHOUT ANY WARRANTY; without even the implied warranty of\n",
            $prefix,
            " MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the\n",
            $prefix,
            " GNU General Public License for more details.\n",
            $prefix,
            "\n",
            $prefix,
            " You should have received a copy of the GNU General Public License\n",
            $prefix,
            " along with this program.  If not, see <https://www.gnu.org/licenses/>.\n",
        )
    };
}

/// This project's header as a Rust file carries it.
const GPL_BLOCK: &str = concat!("// Copyright 2026 Roberto Leinardi.\n", gpl_body!("//"));

/// A Rust source with the header, followed by its module comment. Its last required line is its
/// sixth.
const RUST_HEADER: &str = concat!(
    "// Copyright 2026 Roberto Leinardi.\n",
    gpl_body!("//"),
    "\n//! A module.\n"
);

/// The provenance comment of a reused Rust file, on one line.
const PROVENANCE_LINE: &str = concat!(
    "// Adapted from swarm-scheduler-exporter/src/clock.rs at d57dcc220d29, ",
    "originally MIT-licensed, Copyright (c) 2026 Roberto Leinardi\n"
);

/// The same comment wrapped over three lines, as a long path forces it to be.
const PROVENANCE_WRAPPED: &str = concat!(
    "// Adapted from swarm-device-access/meta/tests/license_headers.rs\n",
    "// at 4cd31c4, originally Apache-2.0-licensed,\n",
    "// Copyright (c) 2026 Roberto Leinardi\n"
);

/// Another licence altogether, with the right copyright line.
const MIT_HEADER: &str = concat!(
    "// Copyright 2026 Roberto Leinardi.\n",
    "//\n",
    "// Permission is hereby granted, free of charge, to any person obtaining a copy\n",
    "\n",
    "fn main() {}\n"
);

/// The opening of the MIT licence's permission paragraph.
const MIT_PERMISSION: &str =
    "// Permission is hereby granted, free of charge, to any person obtaining a copy\n";

/// The first line of the Apache-2.0 notice.
const APACHE_NOTICE: &str =
    "// Licensed under the Apache License, Version 2.0 (the \x22License\x22);\n";

const SHELL_HEADER: &str = concat!(
    "#!/usr/bin/env bash\n",
    "#\n",
    "# Copyright 2026 Roberto Leinardi.\n",
    gpl_body!("#"),
    "echo ok\n"
);

const MAKEFILE: &str = "MK_COMMON_FILES := help.mk pre-commit.mk\n";

/// The scratch repositories' Makefile, which the check governs too.
const MAKEFILE_WITH_HEADER: &str = concat!(
    "#\n# Copyright 2026 Roberto Leinardi.\n",
    gpl_body!("#"),
    "MK_COMMON_FILES := help.mk pre-commit.mk\n"
);

fn run_git(directory: &Path, args: &[&str]) {
    let output = gitenv::isolate(Command::new("git").args(args).current_dir(directory))
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A git repository holding the checker, a Makefile (unless the case brings its own) and `files`,
/// every one of them added except those under [`UNTRACKED_PREFIX`].
fn scratch_repo(files: &[(&str, String)]) -> tempfile::TempDir {
    let scratch = tempfile::tempdir().unwrap_or_else(|error| panic!("scratch directory: {error}"));
    let directory = scratch.path();
    let mut files = files.to_vec();
    files.push((SCRIPT_PATH, repo::read(SCRIPT_PATH)));
    if !files.iter().any(|(name, _)| *name == "Makefile") {
        files.push(("Makefile", MAKEFILE_WITH_HEADER.to_owned()));
    }

    for (name, content) in &files {
        let path = directory.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create the directory of {name}: {error}"));
        }
        fs::write(&path, content).unwrap_or_else(|error| panic!("write {name}: {error}"));
    }
    fs::set_permissions(
        directory.join(SCRIPT_PATH),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap_or_else(|error| panic!("make the checker executable: {error}"));

    run_git(directory, &["init", "--quiet"]);
    for (name, _) in &files {
        if !name.starts_with(UNTRACKED_PREFIX) {
            run_git(directory, &["add", "--", name]);
        }
    }
    scratch
}

/// Runs the checker kept at [`SCRIPT_PATH`] inside the repository at `directory`.
fn run_checker(directory: &Path) -> Output {
    gitenv::isolate(Command::new(directory.join(SCRIPT_PATH)).current_dir(directory))
        .output()
        .unwrap_or_else(|error| panic!("run the checker: {error}"))
}

/// The repository passes its own check. Run with the repository's own git environment: under the
/// commit hook it names the index being committed.
#[test]
fn repository_headers() {
    let output = Command::new(repo::path(SCRIPT_PATH))
        .current_dir(repo::root())
        .output()
        .expect("run the checker");
    assert!(
        output.status.success(),
        "the repository fails its own license header check:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One scratch repository: its files, and whether the checker must accept it.
struct HeaderCase {
    name: &'static str,
    files: Vec<(&'static str, String)>,
    pass: bool,
}

fn case(name: &'static str, files: &[(&'static str, &str)], pass: bool) -> HeaderCase {
    HeaderCase {
        name,
        files: files
            .iter()
            .map(|(path, content)| (*path, (*content).to_owned()))
            .collect(),
        pass,
    }
}

fn run_cases(cases: Vec<HeaderCase>) {
    let mut failures = Vec::new();
    for HeaderCase { name, files, pass } in cases {
        let scratch = scratch_repo(&files);
        let output = run_checker(scratch.path());
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if pass && !output.status.success() {
            failures.push(format!("{name}: rejected, want accepted:\n{text}"));
        }
        if !pass && output.status.success() {
            failures.push(format!("{name}: accepted, want rejected:\n{text}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The edges of the window the header must sit in. `RUST_HEADER`'s last required line is its
/// sixth: 19 lines before it put it on line 25, the last line of the window, and one more pushes
/// it out.
#[test]
fn header_cases_for_the_window() {
    let at_the_edge = format!("{}{RUST_HEADER}", "// filler\n".repeat(19));
    let past_the_edge = format!("{}{RUST_HEADER}", "// filler\n".repeat(20));
    run_cases(vec![
        case(
            "header ending on the last line of the window",
            &[("src/x.rs", &at_the_edge)],
            true,
        ),
        case(
            "header ending one line past the window",
            &[("src/x.rs", &past_the_edge)],
            false,
        ),
    ]);
}

/// Every other governed file kind with and without the header, and what is never governed:
/// untracked files, make-common's files, Markdown, YAML, TOML, Blueprint, Meson and data files.
#[test]
fn header_cases_for_other_kinds_and_exemptions() {
    let year_range = SHELL_HEADER.replace("Copyright 2026", "Copyright 2025-2026");
    run_cases(vec![
        case(
            "shell script with the header",
            &[("scripts/a.sh", SHELL_HEADER)],
            true,
        ),
        case(
            "shell script with a year range",
            &[("scripts/a.sh", &year_range)],
            true,
        ),
        case(
            "shell script without a header",
            &[("scripts/a.sh", "#!/usr/bin/env bash\necho\n")],
            false,
        ),
        case(
            "shell script under testdata, no header",
            &[("tests/testdata/x.sh", "#!/usr/bin/env bash\necho\n")],
            false,
        ),
        case(
            "local make fragment, no header",
            &[(".mk/local.mk", "x:\n")],
            false,
        ),
        case(
            "makefile without a header",
            &[("Makefile", MAKEFILE)],
            false,
        ),
        case(
            "untracked file without a header",
            &[("untracked/scratch.sh", "#!/usr/bin/env bash\necho\n")],
            true,
        ),
        case(
            "upstream make fragment",
            &[(".mk/help.mk", "x:\n"), (".mk/pre-commit.mk", "y:\n")],
            true,
        ),
        case(
            "bootstrap script",
            &[("scripts/bootstrap-mk-common.sh", "#!/bin/sh\n")],
            true,
        ),
        case(
            "markdown and yaml",
            &[("README.md", "# x\n"), ("a.yaml", "a: 1\n")],
            true,
        ),
        case(
            "cargo manifest and toml",
            &[("Cargo.toml", "[package]\n"), ("clippy.toml", "x = 1\n")],
            true,
        ),
        case(
            "blueprint, meson and data files",
            &[
                ("data/resources/ui/window.blp", "using Gtk 4.0;\n"),
                ("meson.build", "project('x')\n"),
                ("data/x.desktop.in.in", "[Desktop Entry]\n"),
                ("build-aux/x.json", "{}\n"),
            ],
            true,
        ),
    ]);
}

/// Rust: every .rs file of every package is governed, except the fixtures under a testdata/
/// directory.
#[test]
fn header_cases_for_rust() {
    let other_holder = RUST_HEADER.replace("Roberto Leinardi", "Someone Else");
    let gpl_two = RUST_HEADER.replace(
        "either version 3 of the License",
        "version 2 of the License",
    );
    run_cases(vec![
        case(
            "rust file with the header",
            &[("src/x.rs", RUST_HEADER)],
            true,
        ),
        case(
            "rust file without a header",
            &[("src/x.rs", "//! A module.\n")],
            false,
        ),
        case(
            "rust file with another license",
            &[("src/x.rs", MIT_HEADER)],
            false,
        ),
        case(
            "rust file under the gpl version 2",
            &[("src/x.rs", &gpl_two)],
            false,
        ),
        case(
            "rust test without a header",
            &[("tests/run.rs", "fn main() {}\n")],
            false,
        ),
        case(
            "rust build script, no header",
            &[("build.rs", "fn main() {}\n")],
            false,
        ),
        case(
            "meta crate file without a header",
            &[("meta/tests/x.rs", "fn main() {}\n")],
            false,
        ),
        case(
            "x52 crate file without a header",
            &[("x52/src/lib.rs", "//! x\n")],
            false,
        ),
        case(
            "another holder on a rust file",
            &[("src/x.rs", &other_holder)],
            false,
        ),
        case(
            "rust fixture under testdata",
            &[("tests/testdata/fixture.rs", "fn main() {}\n")],
            true,
        ),
        case(
            "rust file next to testdata",
            &[("tests/testdatax/x.rs", "fn main() {}\n")],
            false,
        ),
    ]);
}

/// The provenance comment of a reused file: accepted, not required, and complete if present; and
/// the MIT permission text and the Apache-2.0 notice refused.
#[test]
fn header_cases_for_the_provenance_comment() {
    let one_line = format!("{GPL_BLOCK}//\n{PROVENANCE_LINE}\n//! The clock.\n");
    let wrapped = format!("{GPL_BLOCK}//\n{PROVENANCE_WRAPPED}\n//! The test.\n");
    let without_commit = format!(
        "{GPL_BLOCK}{}",
        PROVENANCE_LINE.replacen(" at d57dcc220d29", "", 1)
    );
    let other_holder = format!(
        "{GPL_BLOCK}{}",
        PROVENANCE_LINE.replacen("Roberto Leinardi", "Someone Else", 1)
    );
    let other_licence = format!(
        "{GPL_BLOCK}{}",
        PROVENANCE_LINE.replacen("MIT-licensed", "BSD-3-Clause-licensed", 1)
    );
    let cut_short = format!(
        "{GPL_BLOCK}// Adapted from swarm-scheduler-exporter/src/clock.rs\n\n//! The clock.\n"
    );
    let without_header = format!("{PROVENANCE_LINE}\n//! The clock.\n");
    let pasted_mit = format!("{GPL_BLOCK}{PROVENANCE_LINE}//\n{MIT_PERMISSION}");
    let pasted_apache = format!("{GPL_BLOCK}{PROVENANCE_LINE}//\n{APACHE_NOTICE}");
    run_cases(vec![
        case(
            "provenance on one line",
            &[("src/clock.rs", &one_line)],
            true,
        ),
        case("provenance wrapped", &[("meta/tests/x.rs", &wrapped)], true),
        case(
            "provenance without a commit",
            &[("src/clock.rs", &without_commit)],
            false,
        ),
        case(
            "provenance with another holder",
            &[("src/clock.rs", &other_holder)],
            false,
        ),
        case(
            "provenance from another licence",
            &[("src/clock.rs", &other_licence)],
            false,
        ),
        case(
            "provenance cut short",
            &[("src/clock.rs", &cut_short)],
            false,
        ),
        case(
            "provenance without the header",
            &[("src/clock.rs", &without_header)],
            false,
        ),
        case(
            "pasted mit block on a rust file",
            &[("src/clock.rs", &pasted_mit)],
            false,
        ),
        case(
            "pasted apache notice on a rust file",
            &[("src/clock.rs", &pasted_apache)],
            false,
        ),
    ]);
}
