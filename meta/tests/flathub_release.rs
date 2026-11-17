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

//! Drives scripts/flathub-release.sh against local stand-ins: a bare repository for
//! flathub/com.leinardi.gx52 holding the legacy tree, an upstream repository with release tags, a
//! directory of release assets served through file://, and a fake gh that keeps its pull requests
//! in a directory. The pull request branch carries exactly the release's files and its pull request
//! is opened once and then updated, `beta` is created once from the passed test build (closing its
//! pull request), and a manifest that does not name the tag's commit is refused before anything is
//! pushed.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use meta::repo;
use meta::scratch::{Run, Scratch};

const SCRIPT: &str = "scripts/flathub-release.sh";
const FLATHUB_JSON: &str = "build-aux/flathub/flathub.json";
const MANIFEST: &str = "com.leinardi.gx52.json";

/// The legacy Python manifest: no gx52 module vendoring cargo-sources.json.
const LEGACY_MANIFEST: &str =
    r#"{"id": "com.leinardi.gx52", "modules": ["python3-setuptools.json"]}"#;

/// A gh that knows `pr list`, `pr create`, `pr edit`, `pr close`, `pr checks`, `pr view` and
/// `pr merge`: each call is a line of `GH_STATE/log` (`action|url|head|base|title`), an open pull
/// request is `GH_STATE/open/N` (holding `head base`), its body `GH_STATE/body-N`, its checks
/// `GH_STATE/checks-N` (`bucket name` lines) and its comments `GH_STATE/comments-N`. Its head commit
/// is `HEAD_COMMIT`; `pr merge` logs the commit it is pinned to (`--match-head-commit`) as `head`.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
state="${GH_STATE:?}"
mkdir -p "$state/open" "$state/closed" "$state/merged"
action="$1 $2"
shift 2
url="" head="" base="" title="" body="" json=""
case "$action" in
"pr list" | "pr create") ;;
*)
	url="$1"
	shift
	;;
esac
while (($#)); do
	case "$1" in
	--head) head="$2" ;;
	--base) base="$2" ;;
	--title) title="$2" ;;
	--body) body="$2" ;;
	--json) json="$2" ;;
	--match-head-commit) head="$2" ;;
	--merge) shift && continue ;;
	esac
	shift 2
done
echo "$action|$url|$head|$base|$title" >>"$state/log"
prefix=https://github.com/flathub/com.leinardi.gx52/pull
number="${url##*/}"
case "$action" in
"pr list")
	for file in "$state"/open/*; do
		[[ -f "$file" ]] || continue
		read -r open_head open_base <"$file"
		if [[ -n "$head" && "$open_head $open_base" == "$head $base" ]]; then
			echo "$prefix/${file##*/}"
		elif [[ -z "$head" && "$open_base" == "$base" ]]; then
			echo "$prefix/${file##*/} $open_head"
		fi
	done
	;;
"pr create")
	number=$(($(cat "$state/count" 2>/dev/null || echo 0) + 1))
	echo "$number" >"$state/count"
	echo "$head $base" >"$state/open/$number"
	printf '%s' "$body" >"$state/body-$number"
	echo "$prefix/$number"
	;;
"pr edit") printf '%s' "$body" >"$state/body-$number" ;;
"pr close") mv "$state/open/$number" "$state/closed/" ;;
"pr checks") cat "$state/checks-$number" 2>/dev/null || true ;;
"pr view")
	if [[ "$json" == headRefOid ]]; then
		echo 0123456789abcdef0123456789abcdef01234567
	else
		cat "$state/comments-$number" 2>/dev/null || true
	fi
	;;
"pr merge") mv "$state/open/$number" "$state/merged/" ;;
*) exit 2 ;;
esac
"#;

/// The release tree's files, in `git ls-files` order.
const RELEASE_TREE: [&str; 4] = [
    "README.md",
    "cargo-sources.json",
    "com.leinardi.gx52.json",
    "flathub.json",
];

struct Setup {
    /// The gx52 checkout the script runs from.
    gx52: Scratch,
    upstream: Scratch,
    /// The working repository behind the bare Flathub remote, holding its legacy tree.
    legacy: Scratch,
    /// Holds `flathub.git`, the clones directory and the release assets.
    outside: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let gx52 = Scratch::new();
        gx52.copy_script(SCRIPT);
        gx52.write(FLATHUB_JSON, &repo::read(FLATHUB_JSON));

        let legacy = Scratch::new();
        legacy.git(&["switch", "--quiet", "--create", "master"]);
        legacy.write("README.md", "Flathub packaging of GX52.\n");
        legacy.write(MANIFEST, LEGACY_MANIFEST);
        legacy.write("pypi-dependencies.json", "{}\n");
        legacy.write("flathub.json", r#"{"only-arches": ["x86_64"]}"#);
        legacy.commit("Update to 0.7.6");

        let outside = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary dir: {error}"));
        let setup = Self {
            gx52,
            upstream: Scratch::new(),
            legacy,
            outside,
        };
        setup
            .legacy
            .git(&["clone", "--quiet", "--bare", ".", &setup.remote()]);
        let gh = setup.outside.path().join("gh");
        fs::write(&gh, FAKE_GH).unwrap_or_else(|error| panic!("write the fake gh: {error}"));
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("make the fake gh executable: {error}"));
        setup
    }

    fn outside(&self, relative: &str) -> String {
        self.outside.path().join(relative).display().to_string()
    }

    fn remote(&self) -> String {
        self.outside("flathub.git")
    }

    /// Tags a new upstream commit `tag` and publishes its assets; `commit` overrides the commit the
    /// manifest names.
    fn release(&self, tag: &str, commit: Option<&str>) {
        self.upstream.write("version", tag);
        self.upstream.commit(tag);
        self.upstream.tag(tag);
        let head = self.upstream.git_stdout(&["rev-parse", "HEAD"]);
        let manifest = format!(
            r#"{{"id": "com.leinardi.gx52", "modules": [{{"name": "gx52", "sources": [
                {{"type": "git", "url": "https://github.com/leinardi/gx52.git", "tag": "{tag}",
                  "commit": "{}"}},
                "cargo-sources.json"]}}]}}"#,
            commit.unwrap_or(&head)
        );
        let assets = self.outside.path().join("releases/download").join(tag);
        std::fs::create_dir_all(&assets).unwrap_or_else(|error| panic!("assets dir: {error}"));
        for (name, content) in [
            (MANIFEST, manifest.as_str()),
            (
                "cargo-sources.json",
                r#"[{"type": "file", "dest": "cargo/vendor"}]"#,
            ),
        ] {
            std::fs::write(assets.join(name), content)
                .unwrap_or_else(|error| panic!("write {name}: {error}"));
        }
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_with(args, &[])
    }

    fn run_with(&self, args: &[&str], extra: &[(&str, &str)]) -> Run {
        let releases = format!("file://{}", self.outside("releases"));
        let clones = self.outside("clones");
        let remote = self.remote();
        let upstream = self.upstream.path().display().to_string();
        let gh = self.outside("gh");
        let gh_state = self.outside("gh-state");
        let mut env: Vec<(&str, &str)> = vec![
            ("FLATHUB_DIR", clones.as_str()),
            ("FLATHUB_URL", remote.as_str()),
            ("UPSTREAM_URL", upstream.as_str()),
            ("RELEASES_URL", releases.as_str()),
            ("GH", gh.as_str()),
            ("GH_STATE", gh_state.as_str()),
            ("GIT_AUTHOR_NAME", "Maintainer"),
            ("GIT_AUTHOR_EMAIL", "maintainer@example.com"),
            ("GIT_COMMITTER_NAME", "Maintainer"),
            ("GIT_COMMITTER_EMAIL", "maintainer@example.com"),
            ("POLL_SECONDS", "1"),
        ];
        env.extend_from_slice(extra);
        self.gx52.run(SCRIPT, args, &env)
    }

    fn succeeds(&self, args: &[&str]) -> String {
        let run = self.run(args);
        assert!(run.success, "{args:?} failed:\n{}", run.text);
        run.text
    }

    fn refused(&self, args: &[&str], reason: &str) {
        let run = self.run(args);
        assert!(
            !run.success,
            "{args:?} succeeded, want refused:\n{}",
            run.text
        );
        assert!(
            run.text.contains(reason),
            "{args:?}: want '{reason}', got:\n{}",
            run.text
        );
    }

    /// Runs git in the bare remote.
    fn remote_git(&self, args: &[&str]) -> String {
        let remote = self.remote();
        let mut all = vec!["--git-dir", remote.as_str()];
        all.extend_from_slice(args);
        self.legacy.git_stdout(&all)
    }

    fn branches(&self) -> Vec<String> {
        self.remote_git(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn files(&self, branch: &str) -> Vec<String> {
        self.remote_git(&["ls-tree", "-r", "--name-only", branch])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn show(&self, branch: &str, file: &str) -> String {
        self.remote_git(&["show", &format!("{branch}:{file}")])
    }

    /// The fake gh's calls, one `action|url|head|base|title` line each, `pr list` left out.
    fn gh_calls(&self) -> Vec<String> {
        fs::read_to_string(self.outside("gh-state/log"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.starts_with("pr list|"))
            .map(str::to_owned)
            .collect()
    }

    fn body(&self, number: u32) -> String {
        let path = self.outside(&format!("gh-state/body-{number}"));
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    }

    fn open_pull_requests(&self) -> usize {
        fs::read_dir(self.outside("gh-state/open"))
            .map(Iterator::count)
            .unwrap_or_default()
    }
}

#[test]
fn the_migration_branch_carries_exactly_the_release() {
    let setup = Setup::new();
    setup.release("v0.99.0", None);
    let output = setup.succeeds(&["pr", "0.99.0", "master"]);

    let branch = "gx52/v0.99.0-master";
    assert_eq!(setup.branches(), [branch, "master"], "pushed branches");
    assert_eq!(setup.files(branch), RELEASE_TREE, "the branch's tree");
    let assets = setup.outside("releases/download/v0.99.0");
    for (file, want) in [
        (MANIFEST, format!("{assets}/{MANIFEST}")),
        ("cargo-sources.json", format!("{assets}/cargo-sources.json")),
    ] {
        let want = std::fs::read_to_string(&want).unwrap_or_else(|error| panic!("{want}: {error}"));
        assert_eq!(setup.show(branch, file), want.trim_end(), "{file}");
    }
    assert_eq!(
        setup.show(branch, "flathub.json"),
        repo::read(FLATHUB_JSON).trim_end(),
        "flathub.json"
    );
    assert_eq!(
        setup.show(branch, "README.md"),
        "Flathub packaging of GX52.",
        "README.md is master's"
    );
    assert_eq!(
        setup.remote_git(&["rev-parse", &format!("{branch}~1")]),
        setup.remote_git(&["rev-parse", "master"]),
        "the branch is one commit on master"
    );
    let title = "Migrate to the Rust rewrite (v0.99.0): test build for the beta branch";
    assert_eq!(
        setup.gh_calls(),
        [format!("pr create||{branch}|master|{title}")],
        "the pull request"
    );
    assert!(
        output.contains(
            "Opened the pull request: https://github.com/flathub/com.leinardi.gx52/pull/1"
        ),
        "{output}"
    );
    let body = setup.body(1);
    assert!(
        body.starts_with("Release: https://github.com/leinardi/gx52/releases/tag/v0.99.0\n"),
        "the body does not link the GitHub release:\n{body}"
    );
    assert!(body.contains("not to be merged into master"), "{body}");
    assert!(
        output.contains("make flathub-beta VERSION=0.99.0"),
        "{output}"
    );

    // A second run rebuilds the same branch, still one commit on master, and updates the same
    // pull request.
    let again = setup.succeeds(&["pr", "0.99.0", "master"]);
    assert_eq!(
        setup.remote_git(&["rev-list", "--count", &format!("master..{branch}")]),
        "1",
        "a re-run stacked a commit"
    );
    assert_eq!(
        setup.gh_calls().last().map(String::as_str),
        Some(
            format!("pr edit|https://github.com/flathub/com.leinardi.gx52/pull/1|||{title}")
                .as_str()
        ),
        "a re-run did not update the open pull request"
    );
    assert!(again.contains("Updated the pull request"), "{again}");
    assert_eq!(setup.open_pull_requests(), 1);
}

#[test]
fn beta_is_created_once_from_the_test_build() {
    let setup = Setup::new();
    setup.release("v0.99.0", None);
    setup.refused(&["beta", "0.99.0"], "run 'pr 0.99.0 master' first");
    setup.refused(&["pr", "0.99.0", "beta"], "has no beta branch yet");

    setup.succeeds(&["pr", "0.99.0", "master"]);
    let created = setup.succeeds(&["beta", "0.99.0"]);
    assert!(
        created.contains("Closed the test-build pull request: https://github.com/flathub/com.leinardi.gx52/pull/1"),
        "{created}"
    );
    assert_eq!(
        setup.open_pull_requests(),
        0,
        "the test build is still open"
    );
    assert_eq!(
        setup.remote_git(&["rev-parse", "beta"]),
        setup.remote_git(&["rev-parse", "gx52/v0.99.0-master"]),
        "beta is master with the test build merged"
    );
    assert_eq!(
        setup.files("master"),
        [
            "README.md",
            MANIFEST,
            "flathub.json",
            "pypi-dependencies.json"
        ]
    );
    setup.refused(&["beta", "0.99.0"], "already has a beta branch");

    // Nothing changes for the release beta already has; the next one is an update.
    let same = setup.succeeds(&["pr", "0.99.0", "beta"]);
    assert!(same.contains("Nothing to change"), "{same}");
    assert!(!setup.branches().contains(&"gx52/v0.99.0-beta".to_owned()));
    setup.release("v0.99.1", None);
    setup.succeeds(&["pr", "0.99.1", "beta"]);
    assert_eq!(
        setup.gh_calls().last().map(String::as_str),
        Some("pr create||gx52/v0.99.1-beta|beta|Update to v0.99.1"),
    );
    assert_eq!(setup.files("gx52/v0.99.1-beta"), RELEASE_TREE);
}

#[test]
fn a_built_pull_request_is_merged_on_confirmation_only() {
    let setup = Setup::new();
    setup.release("v0.99.0", None);
    setup.refused(&["merge", "beta"], "no open gx52/* pull request into beta");
    setup.succeeds(&["pr", "0.99.0", "master"]);
    setup.refused(&["merge", "master"], "the test build of the beta branch");
    setup.succeeds(&["beta", "0.99.0"]);
    setup.release("v0.99.1", None);
    setup.succeeds(&["pr", "0.99.1", "beta"]);
    let state = setup.outside("gh-state");
    let write = |file: &str, content: &str| {
        fs::write(format!("{state}/{file}"), content).unwrap_or_else(|error| panic!("{error}"));
    };

    let refused_with = |checks: &str, reason: &str| {
        write("checks-2", checks);
        let run = setup.run_with(
            &["merge", "beta"],
            &[("WAIT_SECONDS", "2"), ("ASSUME_YES", "1")],
        );
        assert!(!run.success, "{checks:?}: merged:\n{}", run.text);
        assert!(
            run.text.contains(reason),
            "{checks:?}: want '{reason}':\n{}",
            run.text
        );
    };
    // Not built yet: no checks, or only the ones that pass before the build starts.
    refused_with("", "flathub/build has not passed");
    refused_with("pass flathub/permissions\n", "flathub/build has not passed");
    refused_with(
        "pass flathub/permissions\npending flathub/build\n",
        "flathub/build has not passed",
    );
    refused_with(
        "pass flathub/permissions\nskipping flathub/build\n",
        "flathub/build was skipped",
    );
    refused_with(
        "pass flathub/permissions\nfail flathub/build\n",
        "fail flathub/build",
    );
    refused_with(
        "fail flathub/permissions\npass flathub/build\n",
        "fail flathub/permissions",
    );
    assert_eq!(
        setup.open_pull_requests(),
        1,
        "an unbuilt pull request was merged"
    );
    assert!(
        !setup
            .gh_calls()
            .iter()
            .any(|call| call.starts_with("pr merge|")),
        "gh pr merge ran"
    );

    write("checks-2", "pass flathub/permissions\npass flathub/build\n");
    write(
        "comments-2",
        "Test build succeeded. To test this build, install it from the testing repository:\n\n```\nflatpak install --user https://dl.flathub.org/build-repo/12345/com.leinardi.gx52.flatpakref\n```\n",
    );
    let declined = setup.run(&["merge", "beta"]);
    assert!(
        !declined.success,
        "merged without a confirmation:\n{}",
        declined.text
    );
    assert!(
        declined.text.contains(
            "flatpak install --user https://dl.flathub.org/build-repo/12345/com.leinardi.gx52.flatpakref"
        ),
        "no test build install command:\n{}",
        declined.text
    );
    assert_eq!(setup.open_pull_requests(), 1, "declined, but merged");

    let merged = setup.run_with(&["merge", "beta"], &[("ASSUME_YES", "1")]);
    assert!(merged.success, "{}", merged.text);
    assert_eq!(
        setup.gh_calls().last().map(String::as_str),
        Some(
            "pr merge|https://github.com/flathub/com.leinardi.gx52/pull/2|0123456789abcdef0123456789abcdef01234567||"
        ),
        "the merge is not pinned to the built commit"
    );
    assert_eq!(setup.open_pull_requests(), 0);
}

#[test]
fn bad_inputs_and_mismatched_assets_are_refused() {
    let setup = Setup::new();
    setup.release("v0.99.0", Some("0123456789abcdef0123456789abcdef01234567"));
    setup.refused(&["pr", "0.99", "master"], "must be X.Y.Z");
    setup.refused(&["pr", "v0.99.0", "master"], "must be X.Y.Z");
    setup.refused(&["pr", "0.99.0", "stable"], "must be beta or master");
    setup.refused(&["pr", "0.98.0", "master"], "does not exist");
    setup.refused(&["pr", "0.99.0", "master"], "does not name v0.99.0");
    setup.refused(&["publish"], "usage");
    assert_eq!(setup.branches(), ["master"], "a refused run pushed");
    assert!(setup.gh_calls().is_empty(), "a refused run called gh");

    setup.release("v0.99.1", None);
    setup.succeeds(&["pr", "0.99.1", "master"]);
    let clone = setup.outside("clones/com.leinardi.gx52");
    std::fs::write(format!("{clone}/stray"), "").unwrap_or_else(|error| panic!("{error}"));
    setup.refused(&["pr", "0.99.1", "master"], "has uncommitted changes");
}
