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

//! Drives scripts/release-policy.sh, the Release workflow's first job, on scratch repositories:
//! before a `v1.0.0` tag an omitted version is refused whatever the commits since the last tag
//! would make svu derive, and an explicit one must be `0.99.x` or `1.0.0`; from `v1.0.0` on an
//! omitted version is allowed. Every refusal has an accepted case next to it.

use meta::scratch::Scratch;

const SCRIPT: &str = "scripts/release-policy.sh";

/// A repository with the legacy tags, `v0.99.0`, and `tags` on later commits.
fn history(tags: &[&str]) -> Scratch {
    let scratch = Scratch::new();
    scratch.copy_script(SCRIPT);
    scratch.commit("chore(repo): initial commit");
    for tag in ["0.7.5", "0.7.6", "v0.99.0"].iter().chain(tags) {
        scratch.commit(&format!("chore(release): {tag}"));
        scratch.tag(tag);
    }
    scratch
}

fn check(scratch: &Scratch, case: &str, version: Option<&str>, want: Result<(), &str>) {
    let args: Vec<&str> = version.into_iter().collect();
    let run = scratch.run(SCRIPT, &args, &[]);
    match want {
        Ok(()) => assert!(run.success, "{case}: refused, want accepted:\n{}", run.text),
        Err(reason) => {
            assert!(
                !run.success,
                "{case}: accepted, want refused:\n{}",
                run.text
            );
            assert!(
                run.text.contains(reason),
                "{case}: want '{reason}', got:\n{}",
                run.text
            );
        }
    }
}

const EXPLICIT: &str = "give the version explicitly before 1.0.0";
const NOT_DEVELOPMENT: &str = "a 0.99.x development build or 1.0.0 itself";

#[test]
fn before_one_zero_the_version_is_explicit() {
    let scratch = history(&[]);
    scratch.commit("feat(leds): svu would make this 0.100.0");
    check(&scratch, "omitted, with a feat", None, Err(EXPLICIT));
    check(&scratch, "empty, with a feat", Some(""), Err(EXPLICIT));
    scratch.commit("feat(profiles)!: svu would make this 1.0.0");
    check(
        &scratch,
        "omitted, with a breaking change",
        None,
        Err(EXPLICIT),
    );

    for version in ["0.99.1", "v0.99.1", "0.99.10", "1.0.0", "v1.0.0"] {
        check(&scratch, version, Some(version), Ok(()));
    }
    for version in ["0.100.0", "1.1.0", "2.0.0", "0.98.0", "0.7.7"] {
        check(&scratch, version, Some(version), Err(NOT_DEVELOPMENT));
    }
    for version in ["1.2", "v0.99.1-rc.1", "0.99.01", "vv0.99.1", "V0.99.1"] {
        check(
            &scratch,
            version,
            Some(version),
            Err("must be X.Y.Z or vX.Y.Z"),
        );
    }
}

#[test]
fn only_a_strict_v_tag_ends_the_development_builds() {
    for tag in ["1.0.0", "v1.0.0-rc.1", "v01.0.0", "release-1.0.0"] {
        let scratch = history(&[tag]);
        scratch.commit("feat(leds): a feature");
        check(
            &scratch,
            &format!("omitted, after the tag {tag}"),
            None,
            Err(EXPLICIT),
        );
    }
}

#[test]
fn from_one_zero_on_svu_may_derive_it() {
    for tag in ["v1.0.0", "v2.3.4"] {
        let scratch = history(&[tag]);
        scratch.commit("feat(leds): a feature");
        check(&scratch, &format!("omitted, after {tag}"), None, Ok(()));
        check(&scratch, &format!("empty, after {tag}"), Some(""), Ok(()));
        check(
            &scratch,
            &format!("explicit, after {tag}"),
            Some("3.0.0"),
            Ok(()),
        );
        check(
            &scratch,
            &format!("malformed, after {tag}"),
            Some("3.0"),
            Err("must be X.Y.Z or vX.Y.Z"),
        );
    }
}
