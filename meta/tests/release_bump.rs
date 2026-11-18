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

//! Drives scripts/release-bump.sh (`make release-bump`) on scratch repositories: a two-member
//! Cargo workspace with no dependencies, and this repository's own meson.build and metainfo with
//! the version reset to `0.0.0` and only the legacy `0.7.x` releases, so the script is tested
//! against the real files it edits.
//!
//! The successful bumps check the four files, the `<release>` entry (the first release's fixed
//! description and the first stable one's, the `feat` and `fix` subjects since the last tag, the
//! removal of an entry that was
//! never published) and that nothing else changed. Every refusal is the negative control of a
//! bump: it fails and leaves the tree exactly as it was.

use std::sync::LazyLock;

use meta::repo;
use meta::scratch::{Run, Scratch};
use regex::Regex;

const SCRIPT: &str = "scripts/release-bump.sh";
const METAINFO: &str = "data/com.leinardi.gx52.metainfo.xml.in.in";

/// 2023-11-14 in UTC: the date every bump here writes.
const EPOCH: &str = "1700000000";
const DATE: &str = "2023-11-14";

const LEGACY: [&str; 7] = [
    "0.7.6", "0.7.5", "0.7.4", "0.7.3", "0.7.2", "0.7.1", "0.7.0",
];

const ROOT_MANIFEST: &str = r#"[workspace]
members = [".", "x52"]
resolver = "3"

[package]
name = "gx52"
# The release workflow sets the version.
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
x52 = { path = "x52" }
"#;

const MEMBER_MANIFEST: &str = r#"[package]
name = "x52"
version = "0.0.0"
edition = "2024"
publish = false
"#;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("invalid pattern {pattern}: {error}"))
}

static RELEASE_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r#"^    <release version="([^"]+)" date="([^"]+)""#));
static ITEM: LazyLock<Regex> = LazyLock::new(|| regex(r"^          <li>(.*)</li>$"));
static MESON_VERSION: LazyLock<Regex> = LazyLock::new(|| regex(r"(?m)^    version: '[^']*',$"));

/// The repository's metainfo with only its legacy `0.7.x` releases, whatever the rewrite has
/// released since.
fn legacy_metainfo() -> String {
    let metainfo = repo::read(METAINFO);
    let mut kept = Vec::new();
    let mut skipping = false;
    for line in metainfo.lines() {
        if skipping {
            skipping = line != "    </release>";
            continue;
        }
        if let Some(captures) = RELEASE_LINE.captures(line)
            && !captures[1].starts_with("0.7.")
        {
            skipping = !line.ends_with("/>");
            continue;
        }
        kept.push(line);
    }
    kept.join("\n") + "\n"
}

/// The `(version, date)` of every `<release>`, newest first.
fn releases(metainfo: &str) -> Vec<(String, String)> {
    metainfo
        .lines()
        .filter_map(|line| RELEASE_LINE.captures(line))
        .map(|captures| (captures[1].to_owned(), captures[2].to_owned()))
        .collect()
}

fn versions(metainfo: &str) -> Vec<String> {
    releases(metainfo)
        .into_iter()
        .map(|(version, _)| version)
        .collect()
}

/// The `<li>` texts of the `<release>` of `version`.
fn items(metainfo: &str, version: &str) -> Vec<String> {
    let start = format!("    <release version=\"{version}\"");
    metainfo
        .lines()
        .skip_while(|line| !line.starts_with(&start))
        .take_while(|line| *line != "    </release>")
        .filter_map(|line| ITEM.captures(line).map(|captures| captures[1].to_owned()))
        .collect()
}

/// The version of the `name` entry of Cargo.lock.
fn locked(scratch: &Scratch, name: &str) -> String {
    let lock = scratch.read("Cargo.lock");
    let entry = format!("name = \"{name}\"\nversion = \"");
    let start = lock
        .find(&entry)
        .unwrap_or_else(|| panic!("Cargo.lock has no {name} entry:\n{lock}"))
        + entry.len();
    lock[start..]
        .split('"')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// A workspace at `0.0.0`, committed, with the legacy tags `0.7.0` … `0.7.6` unless `legacy` is
/// false.
fn workspace(legacy: bool) -> Scratch {
    let scratch = Scratch::new();
    scratch.write("Cargo.toml", ROOT_MANIFEST);
    scratch.write("src/lib.rs", "");
    scratch.write("x52/Cargo.toml", MEMBER_MANIFEST);
    scratch.write("x52/src/lib.rs", "");
    let meson = repo::read("meson.build");
    assert!(
        MESON_VERSION.is_match(&meson),
        "meson.build has no `    version: '...',` line"
    );
    scratch.write(
        "meson.build",
        &MESON_VERSION.replace(&meson, "    version: '0.0.0',"),
    );
    scratch.write(METAINFO, &legacy_metainfo());
    scratch.copy_script(SCRIPT);
    let generated = cargo(&scratch, &["generate-lockfile", "--offline"]);
    assert!(
        generated.success,
        "cargo generate-lockfile: {}",
        generated.text
    );
    scratch.commit("chore(repo): initial commit");
    if legacy {
        for version in LEGACY.iter().rev() {
            scratch.commit(&format!("chore(release): {version}"));
            scratch.tag(version);
        }
    }
    scratch
}

/// Runs cargo in the scratch repository, with the toolchain running these tests.
fn cargo(scratch: &Scratch, args: &[&str]) -> Run {
    let output = std::process::Command::new(cargo_binary())
        .args(args)
        .current_dir(scratch.path())
        .output()
        .unwrap_or_else(|error| panic!("run cargo {args:?}: {error}"));
    Run {
        success: output.status.success(),
        text: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn cargo_binary() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

fn bump(scratch: &Scratch, version: &str) -> Run {
    let cargo = cargo_binary();
    scratch.run(
        SCRIPT,
        &[version],
        &[("CARGO", &cargo), ("SOURCE_DATE_EPOCH", EPOCH)],
    )
}

fn bump_ok(scratch: &Scratch, version: &str) {
    let run = bump(scratch, version);
    assert!(run.success, "bump to {version} failed:\n{}", run.text);
    let changed = scratch.git_stdout(&["diff", "--name-only"]);
    assert_eq!(
        changed,
        format!("Cargo.lock\nCargo.toml\n{METAINFO}\nmeson.build"),
        "bump to {version} changed other files"
    );
    assert!(
        scratch
            .read("Cargo.toml")
            .contains(&format!("\nversion = \"{version}\"\n")),
        "Cargo.toml is not at {version}"
    );
    assert!(
        scratch
            .read("meson.build")
            .contains(&format!("\n    version: '{version}',\n")),
        "meson.build is not at {version}"
    );
    assert_eq!(locked(scratch, "gx52"), version, "Cargo.lock gx52");
    assert_eq!(
        locked(scratch, "x52"),
        "0.0.0",
        "Cargo.lock x52 must keep its version"
    );
    let metainfo = scratch.read(METAINFO);
    assert_eq!(
        releases(&metainfo).first(),
        Some(&(version.to_owned(), DATE.to_owned())),
        "the newest <release> after the bump to {version}"
    );
}

fn with_legacy(newest: &[&str]) -> Vec<String> {
    newest
        .iter()
        .chain(LEGACY.iter())
        .map(|version| (*version).to_owned())
        .collect()
}

#[test]
fn the_first_release_gets_the_fixed_description() {
    let scratch = workspace(true);
    scratch.commit("feat(leds): never listed, there is no v tag yet");
    bump_ok(&scratch, "0.99.0");
    let metainfo = scratch.read(METAINFO);
    assert_eq!(versions(&metainfo), with_legacy(&["0.99.0"]));
    let first = items(&metainfo, "0.99.0");
    assert_eq!(first.len(), 5, "{first:#?}");
    assert!(
        first[0].contains("rewritten in Rust"),
        "the first item: {}",
        first[0]
    );
    assert!(
        metainfo.contains(&format!(
            "    <release version=\"0.99.0\" date=\"{DATE}\">\n      <description translate=\"no\">\n"
        )),
        "the entry is not translated:\n{metainfo}"
    );
    // The legacy history is untouched.
    assert_eq!(
        items(&metainfo, "0.7.6"),
        items(&legacy_metainfo(), "0.7.6")
    );
}

#[test]
fn the_first_stable_release_gets_the_rewrite_notes() {
    let scratch = workspace(true);
    bump_ok(&scratch, "0.99.0");
    scratch.commit("chore(release): bump version to 0.99.0");
    scratch.tag("v0.99.0");
    scratch.commit("fix(device): reopen the device after resume");
    bump_ok(&scratch, "1.0.0");
    scratch.commit("chore(release): bump version to 1.0.0");
    scratch.tag("v1.0.0");

    let metainfo = scratch.read(METAINFO);
    let stable = items(&metainfo, "1.0.0");
    assert_eq!(stable.len(), 7, "{stable:#?}");
    assert_eq!(
        stable[..5],
        items(&metainfo, "0.99.0")[..],
        "the rewrite description"
    );
    assert!(stable[5].contains("installed from source"), "{}", stable[5]);
    assert!(stable[6].contains("X52 (non-Pro)"), "{}", stable[6]);

    // The next release is an ordinary one again.
    scratch.commit("fix(mfd): clamp the clock offsets");
    bump_ok(&scratch, "1.0.1");
    assert_eq!(
        items(&scratch.read(METAINFO), "1.0.1"),
        ["Clamp the clock offsets"]
    );
}

#[test]
fn notes_come_from_feat_and_fix_and_unpublished_entries_go() {
    let scratch = workspace(true);
    bump_ok(&scratch, "0.99.0");
    scratch.commit("chore(release): bump version to 0.99.0");
    scratch.tag("v0.99.0");

    scratch.commit("feat(leds): add a <blink> & \"pulse\" editor");
    scratch.commit("chore(deps): bump something");
    scratch.commit("fix(device)!: reopen the device after resume");
    scratch.commit("docs(readme): words");
    scratch.commit("refactor(mfd): a fix: nothing ships");
    bump_ok(&scratch, "0.99.1");
    let metainfo = scratch.read(METAINFO);
    assert_eq!(versions(&metainfo), with_legacy(&["0.99.1", "0.99.0"]));
    assert_eq!(
        items(&metainfo, "0.99.1"),
        [
            "Add a &lt;blink&gt; &amp; &quot;pulse&quot; editor",
            "Reopen the device after resume",
        ],
    );

    // 0.99.1 is merged, its gate fails, it is never tagged: the next bump drops its entry and
    // lists its changes again.
    scratch.commit("chore(release): bump version to 0.99.1");
    scratch.commit("fix(mfd): clamp clock offsets");
    bump_ok(&scratch, "0.99.2");
    let metainfo = scratch.read(METAINFO);
    assert_eq!(versions(&metainfo), with_legacy(&["0.99.2", "0.99.0"]));
    assert_eq!(
        items(&metainfo, "0.99.2"),
        [
            "Add a &lt;blink&gt; &amp; &quot;pulse&quot; editor",
            "Reopen the device after resume",
            "Clamp clock offsets",
        ],
    );
    assert_eq!(
        items(&metainfo, "0.99.0").len(),
        5,
        "the published 0.99.0 stays"
    );

    // Nothing that ships since the last tag: a maintenance entry.
    scratch.commit("chore(release): bump version to 0.99.2");
    scratch.tag("v0.99.2");
    scratch.commit("chore(deps): bump something else");
    bump_ok(&scratch, "0.99.3");
    assert_eq!(
        items(&scratch.read(METAINFO), "0.99.3"),
        ["Maintenance release"]
    );
}

/// Each refusal fails and leaves the tree as it was.
fn refused(scratch: &Scratch, version: &str, env: &[(&str, &str)], reason: &str) {
    let before = scratch.status();
    let mut all_env = vec![("SOURCE_DATE_EPOCH", EPOCH)];
    let cargo = cargo_binary();
    if !env.iter().any(|(name, _)| *name == "CARGO") {
        all_env.push(("CARGO", &cargo));
    }
    all_env.extend_from_slice(env);
    let run = scratch.run(SCRIPT, &[version], &all_env);
    assert!(
        !run.success,
        "{version} was accepted, want refused ({reason}):\n{}",
        run.text
    );
    assert!(
        run.text.contains(reason),
        "{version}: want '{reason}', got:\n{}",
        run.text
    );
    assert_eq!(
        scratch.status(),
        before,
        "{version}: the refusal changed the tree"
    );
}

#[test]
fn refusals_leave_the_tree_unchanged() {
    let scratch = workspace(true);
    for (version, reason) in [
        ("0.7.6", "not higher than the highest release, 0.7.6"),
        ("0.7.0", "not higher than the highest release, 0.7.6"),
        ("1.2", "must be X.Y.Z"),
        ("v0.99.0", "must be X.Y.Z"),
        ("0.99.0-rc1", "must be X.Y.Z"),
        ("01.0.0", "must be X.Y.Z"),
        ("", "must be X.Y.Z"),
    ] {
        refused(&scratch, version, &[], reason);
    }

    // A failure after writing (here cargo itself) restores the four files.
    refused(&scratch, "0.99.0", &[("CARGO", "false")], "restored");

    // An uncommitted change to one of the four files.
    scratch.write("meson.build", &(scratch.read("meson.build") + "# local\n"));
    refused(&scratch, "0.99.0", &[], "must have no uncommitted changes");
    scratch.git(&["checkout", "--", "meson.build"]);

    // Lower than, or equal to, a v tag.
    scratch.tag("v0.99.5");
    refused(
        &scratch,
        "0.99.4",
        &[],
        "not higher than the highest release, 0.99.5",
    );
    refused(
        &scratch,
        "0.99.5",
        &[],
        "not higher than the highest release, 0.99.5",
    );
}

#[test]
fn an_unrelated_change_is_refused_before_and_after_the_write() {
    // Before: a tracked file outside the four has an uncommitted change.
    let scratch = workspace(true);
    scratch.write("src/lib.rs", "// local\n");
    refused(&scratch, "0.99.0", &[], "must have no uncommitted changes");
    assert_eq!(scratch.read("src/lib.rs"), "// local\n");

    // After: the resolve changes another tracked file (a cargo that touches src/lib.rs).
    let scratch = workspace(true);
    let wrapper_dir =
        tempfile::tempdir().unwrap_or_else(|error| panic!("wrapper directory: {error}"));
    let wrapper = wrapper_dir.path().join("cargo");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho '// touched' >> src/lib.rs\nexec '{}' \"$@\"\n",
            cargo_binary()
        ),
    )
    .unwrap_or_else(|error| panic!("write the cargo wrapper: {error}"));
    std::fs::set_permissions(
        &wrapper,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap_or_else(|error| panic!("make the cargo wrapper executable: {error}"));
    let wrapper = wrapper.to_string_lossy().into_owned();
    refused(
        &scratch,
        "0.99.0",
        &[("CARGO", &wrapper)],
        "expected Cargo.lock Cargo.toml",
    );
    assert!(
        scratch.read("src/lib.rs").is_empty(),
        "src/lib.rs is not restored"
    );
}

#[test]
fn a_lock_that_moves_more_than_gx52_is_refused() {
    // Cargo.lock says x52 is 0.0.1, its manifest 0.0.0: the resolve rewrites both entries.
    let scratch = workspace(true);
    let lock = scratch.read("Cargo.lock");
    let stale = lock.replacen(
        "name = \"x52\"\nversion = \"0.0.0\"",
        "name = \"x52\"\nversion = \"0.0.1\"",
        1,
    );
    assert_ne!(
        stale, lock,
        "the scratch Cargo.lock has no x52 entry to make stale"
    );
    scratch.write("Cargo.lock", &stale);
    scratch.commit("chore(deps): a stale lock");
    refused(
        &scratch,
        "0.99.0",
        &[],
        "changed more than the root package's version",
    );
    assert_eq!(
        locked(&scratch, "x52"),
        "0.0.1",
        "Cargo.lock is not restored"
    );
}

#[test]
fn the_legacy_floor_holds_without_the_legacy_tags() {
    let scratch = workspace(false);
    refused(
        &scratch,
        "0.7.5",
        &[],
        "not higher than the highest release, 0.7.6",
    );
    bump_ok(&scratch, "0.7.7");
}

#[test]
fn the_carried_version_is_a_floor() {
    let scratch = workspace(true);
    bump_ok(&scratch, "0.99.3");
    scratch.commit("chore(release): bump version to 0.99.3");
    refused(
        &scratch,
        "0.99.3",
        &[],
        "not higher than the version Cargo.toml carries, 0.99.3",
    );
    refused(
        &scratch,
        "0.99.2",
        &[],
        "not higher than the version Cargo.toml carries, 0.99.3",
    );
}
