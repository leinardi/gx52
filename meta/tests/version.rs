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

//! One version source (docs/release.md): the root package's version in `Cargo.toml`, its
//! `Cargo.lock` entry and the Meson project version always agree, and once the release workflow
//! has set a real version, the newest `<release>` of the metainfo names it too. Until then they
//! stay `0.0.0`, which has no `<release>`: the metainfo only holds the history of the legacy
//! `0.7.x` application.

use std::sync::LazyLock;

use meta::repo;
use regex::Regex;

const CARGO_MANIFEST: &str = "Cargo.toml";
const CARGO_LOCK: &str = "Cargo.lock";
const MESON_BUILD: &str = "meson.build";
const METAINFO: &str = "data/com.leinardi.gx52.metainfo.xml.in.in";

/// The version of a build the release workflow has not versioned yet.
const UNRELEASED: &str = "0.0.0";

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("invalid pattern {pattern}: {error}"))
}

/// The `version:` keyword of the `project()` call that opens meson.build.
static MESON_VERSION: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?s)\A\s*(?:#[^\n]*\n\s*)*project\((.*?)\n\)"));
static MESON_VERSION_KEYWORD: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?m)^\s*version:\s*'([^']*)',?\s*$"));
/// A `<release>` start tag, whatever the order of its attributes.
static RELEASE_TAG: LazyLock<Regex> = LazyLock::new(|| regex(r"<release\s[^>]*>"));
static VERSION_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| regex(r#"\sversion="([^"]*)""#));

/// The root package's version, or a problem.
fn cargo_version(manifest: &str) -> Result<String, String> {
    let table: toml::Table = manifest
        .parse()
        .map_err(|error| format!("parse {CARGO_MANIFEST}: {error}"))?;
    table
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{CARGO_MANIFEST} has no [package] version"))
}

/// The version of the `gx52` entry of Cargo.lock, or a problem.
fn lock_version(lock: &str) -> Result<String, String> {
    let table: toml::Table = lock
        .parse()
        .map_err(|error| format!("parse {CARGO_LOCK}: {error}"))?;
    let versions: Vec<&str> = table
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some("gx52"))
        .filter_map(|package| package.get("version").and_then(toml::Value::as_str))
        .collect();
    match versions.as_slice() {
        [version] => Ok((*version).to_owned()),
        found => Err(format!(
            "{CARGO_LOCK} has {} gx52 entries with a version, expected 1",
            found.len()
        )),
    }
}

/// The version of meson.build's `project()` call, or a problem.
fn meson_version(build: &str) -> Result<String, String> {
    let arguments = MESON_VERSION
        .captures(build)
        .and_then(|captures| captures.get(1))
        .ok_or_else(|| format!("{MESON_BUILD} does not open with a project() call"))?;
    let versions: Vec<&str> = MESON_VERSION_KEYWORD
        .captures_iter(arguments.as_str())
        .filter_map(|captures| captures.get(1).map(|version| version.as_str()))
        .collect();
    match versions.as_slice() {
        [version] => Ok((*version).to_owned()),
        [] => Err(format!("{MESON_BUILD}'s project() sets no version")),
        found => Err(format!(
            "{MESON_BUILD}'s project() sets the version {} times",
            found.len()
        )),
    }
}

/// The version of the first, newest, `<release>` of the metainfo, if it has one.
fn newest_release(metainfo: &str) -> Option<String> {
    let tag = RELEASE_TAG.find(metainfo)?;
    VERSION_ATTRIBUTE
        .captures(tag.as_str())
        .map(|captures| captures[1].to_owned())
}

/// Every disagreement between the four files' versions.
fn version_problems(manifest: &str, lock: &str, build: &str, metainfo: &str) -> Vec<String> {
    let (cargo, locked, meson) = match (
        cargo_version(manifest),
        lock_version(lock),
        meson_version(build),
    ) {
        (Ok(cargo), Ok(locked), Ok(meson)) => (cargo, locked, meson),
        (cargo, locked, meson) => {
            return [cargo.err(), locked.err(), meson.err()]
                .into_iter()
                .flatten()
                .collect();
        }
    };
    if cargo != locked {
        return vec![format!(
            "{CARGO_MANIFEST} says {cargo}, but {CARGO_LOCK}'s gx52 entry says {locked}"
        )];
    }
    if cargo != meson {
        return vec![format!(
            "{CARGO_MANIFEST} says {cargo}, but {MESON_BUILD} says {meson}"
        )];
    }
    if cargo == UNRELEASED {
        return Vec::new();
    }
    match newest_release(metainfo) {
        Some(release) if release == cargo => Vec::new(),
        Some(release) => vec![format!(
            "the newest <release> of {METAINFO} is {release}, but the version is {cargo}"
        )],
        None => vec![format!(
            "{METAINFO} has no <release> for the version {cargo}"
        )],
    }
}

#[test]
fn the_version_has_one_source() {
    let problems = version_problems(
        &repo::read(CARGO_MANIFEST),
        &repo::read(CARGO_LOCK),
        &repo::read(MESON_BUILD),
        &repo::read(METAINFO),
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The readings the check rests on find the real files' versions, so it cannot pass by reading
/// nothing.
#[test]
fn the_readings_find_the_versions() {
    assert!(
        cargo_version(&repo::read(CARGO_MANIFEST)).is_ok(),
        "{CARGO_MANIFEST}: no version read"
    );
    assert!(
        lock_version(&repo::read(CARGO_LOCK)).is_ok(),
        "{CARGO_LOCK}: no gx52 version read"
    );
    assert!(
        meson_version(&repo::read(MESON_BUILD)).is_ok(),
        "{MESON_BUILD}: no version read"
    );
    assert!(
        newest_release(&repo::read(METAINFO)).is_some(),
        "{METAINFO}: no <release> read"
    );
}

/// A root manifest at `version`.
fn manifest(version: &str) -> String {
    format!(
        "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"gx52\"\n\
         version = \"{version}\"\n"
    )
}

/// A Cargo.lock whose gx52 entry is at `version`, next to a member at `0.0.0`.
fn lock(version: &str) -> String {
    format!(
        "version = 4\n\n[[package]]\nname = \"gx52\"\nversion = \"{version}\"\n\n\
         [[package]]\nname = \"x52\"\nversion = \"0.0.0\"\n"
    )
}

/// A meson.build whose `project()` is at `version`.
fn build(version: &str) -> String {
    format!(
        "project(\n    'gx52',\n    'rust',\n    version: '{version}',\n    \
         meson_version: '>= 1.1',\n)\n\ndependency('gtk4', version: '>= 4.22')\n"
    )
}

/// A metainfo with these `<release>` entries, in this order.
fn metainfo(releases: &[&str]) -> String {
    let entries: Vec<String> = releases
        .iter()
        .map(|version| format!("    <release version=\"{version}\" date=\"2026-11-02\"/>\n"))
        .collect();
    let entries = entries.concat();
    format!("<component>\n  <releases>\n{entries}  </releases>\n</component>\n")
}

fn legacy() -> String {
    metainfo(&["0.7.6", "0.7.5"])
}

/// `0.0.0` with only older releases, a released version with its entry on top, and a release
/// whose attributes are in another order are accepted.
#[test]
fn version_rules_accept() {
    for (name, problems) in [
        (
            "unreleased, only the legacy history",
            version_problems(
                &manifest("0.0.0"),
                &lock("0.0.0"),
                &build("0.0.0"),
                &legacy(),
            ),
        ),
        (
            "released, newest entry first",
            version_problems(
                &manifest("1.0.0"),
                &lock("1.0.0"),
                &build("1.0.0"),
                &metainfo(&["1.0.0", "0.7.6"]),
            ),
        ),
        (
            "released, date before version",
            version_problems(
                &manifest("1.0.0"),
                &lock("1.0.0"),
                &build("1.0.0"),
                "<releases>\n  <release date=\"2026-11-02\" version=\"1.0.0\">\n</releases>\n",
            ),
        ),
    ] {
        assert_eq!(problems, Vec::<String>::new(), "{name}");
    }
}

/// The negative control: a Cargo.lock entry or a Meson version that differs, at `0.0.0` or not,
/// a newest release that lags or is missing once the version is real, a `project()` without a
/// version, and a manifest or lock without the version are each refused.
#[test]
fn version_rules_refuse() {
    for (name, problems) in [
        (
            "meson differs while unreleased",
            version_problems(
                &manifest("0.0.0"),
                &lock("0.0.0"),
                &build("0.99.0"),
                &legacy(),
            ),
        ),
        (
            "meson differs once released",
            version_problems(
                &manifest("1.0.0"),
                &lock("1.0.0"),
                &build("1.0.1"),
                &metainfo(&["1.0.0"]),
            ),
        ),
        (
            "the newest release lags",
            version_problems(
                &manifest("1.0.1"),
                &lock("1.0.1"),
                &build("1.0.1"),
                &metainfo(&["1.0.0", "1.0.1"]),
            ),
        ),
        (
            "no release for a real version",
            version_problems(
                &manifest("1.0.0"),
                &lock("1.0.0"),
                &build("1.0.0"),
                "<component/>\n",
            ),
        ),
        (
            "a project() without a version",
            version_problems(
                &manifest("0.0.0"),
                &lock("0.0.0"),
                "project(\n    'gx52',\n    'rust',\n)\n\ndependency('gtk4', version: '>= 4.22')\n",
                &legacy(),
            ),
        ),
        (
            "the Cargo.lock entry lags",
            version_problems(
                &manifest("1.0.1"),
                &lock("1.0.0"),
                &build("1.0.1"),
                &metainfo(&["1.0.1"]),
            ),
        ),
        (
            "no gx52 entry in Cargo.lock",
            version_problems(
                &manifest("0.0.0"),
                "version = 4\n\n[[package]]\nname = \"x52\"\nversion = \"0.0.0\"\n",
                &build("0.0.0"),
                &legacy(),
            ),
        ),
        (
            "no package version",
            version_problems("[workspace]\n", &lock("0.0.0"), &build("0.0.0"), &legacy()),
        ),
    ] {
        assert_eq!(problems.len(), 1, "{name}: {problems:#?}");
    }
}
