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

//! Drives scripts/release.sh (`make release`) against a fake gh that plays GitHub: the release
//! workflow opens the bump pull request and dispatches its CI, merging it runs release-publish.yaml,
//! which publishes the release. Knob files make a run fail, a re-run fail or pass, and the trunk
//! move on. What is checked is the recovery: a run that stopped is resumed by the same command,
//! which never dispatches or merges twice, and a release goes to the Flathub branches it names.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use meta::scratch::{Run, Scratch};

const SCRIPT: &str = "scripts/release.sh";
const TRUNK: &str = "main";

/// GitHub, as far as release.sh asks it. State in `GH_STATE`: `release` (the release exists, holding
/// its `isDraft`: `true` or `false`), `pr-open` (`number head url`), `pr-merged` (`merge url`), the
/// bump branch in `pr-open-branch` and `pr-merged-branch`, its author in `pr-open-author` and
/// `pr-merged-author` (default github-actions[bot]), `runs/N` (`workflow|commit|branch|event|
/// status|conclusion|created|url`). Knobs: `dispatch-result`, `ci-result`, `publish-result` (a new
/// run's conclusion, default success), `publish-leaves-draft` (a failed publish leaves a draft
/// release), `rerun-result` (a re-run's, default success), `behind` (how far the bump branch is
/// behind the trunk, default 0; a dispatch resets it), `derived` (the version a dispatch with no
/// version input derives, default 1.1.0) and `dispatch-late` (a dry run on the trunk shows up right
/// after the next release.yaml dispatch, whose own run shows only from the second look at it).
/// The pulls API answers with what release.sh's jq expression would print, picked by the fields it
/// names. Every call but the reads is a line of `log`, a dispatch with its version input (`-` when
/// none).
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
state="${GH_STATE:?}"
mkdir -p "$state/runs"
knob() { cat "$state/$1" 2>/dev/null || echo "$2"; }
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
next_id() { echo $(($(ls "$state/runs" | wc -l) + 1)); }
add_run() { # workflow commit branch event conclusion [id]
	local id="${6:-$(next_id)}"
	echo "$1|$2|$3|$4|completed|$5|$(now)|https://github.com/leinardi/gx52/actions/runs/$id" >"$state/runs/$id"
	echo "$id"
}
line() { # a runs/N file as "id status conclusion created url"
	IFS='|' read -r _ _ _ _ status conclusion created url <"$state/runs/$1"
	echo "$1 $status $conclusion $created $url"
}
dispatched() { # ref version id: a release.yaml run, and on the trunk the bump pull request
	local ref="$1" version="$2" result count id
	result="$(knob dispatch-result success)"
	id="$(add_run release.yaml "" "$ref" workflow_dispatch "$result" "$3")"
	if [[ "$result" == success && "$ref" == main ]]; then
		count=$(($(knob dispatches 0) + 1))
		echo "$count" >"$state/dispatches"
		[[ "$version" != - ]] || version="$(knob derived 1.1.0)"
		echo "28 head$count https://github.com/leinardi/gx52/pull/28" >"$state/pr-open"
		echo "release/v$version" >"$state/pr-open-branch"
		echo "github-actions[bot]" >"$state/pr-open-author"
		rm -f "$state/behind"
		add_run ci.yaml "head$count" "release/v$version" workflow_dispatch "$(knob ci-result success)" >/dev/null
	fi
	echo "$id"
}
pulls() { # state head jq: what bump_pulls' jq would print
	local prstate="$1" head="$2" jq="$3" file branch
	[[ "$prstate" == open ]] && file=pr-open || file=pr-merged
	[[ -f "$state/$file" ]] || return 0
	[[ "$(knob "$file-author" "github-actions[bot]")" == "github-actions[bot]" ]] || return 0
	branch="$(cat "$state/$file-branch")"
	[[ -z "$head" || "$branch" == "${head#*:}" ]] || return 0
	case "$jq" in
	*merge_commit_sha*) cat "$state/$file" ;;
	*.number*) cat "$state/$file" ;;
	*merged_at*) echo "2026-01-01T00:00:00Z ${branch#release/v}" ;;
	*) echo "${branch#release/v}" ;;
	esac
}
if [[ "$1" == api ]]; then
	shift
	endpoint="" jq="" ref="" version="-"
	while (($#)); do
		case "$1" in
		-X) shift ;;
		--paginate) ;;
		--jq) jq="$2"; shift ;;
		-f | -F)
			case "$2" in
			ref=*) ref="${2#ref=}" ;;
			"inputs[version]="*) version="${2#*=}" ;;
			esac
			shift
			;;
		*) endpoint="$1" ;;
		esac
		shift
	done
	case "$endpoint" in
	*/compare/*) knob behind 0 ;;
	*/actions/workflows/release.yaml/dispatches)
		echo "workflow run release.yaml $ref $version" >>"$state/log"
		if [[ -f "$state/dispatch-late" ]]; then
			rm "$state/dispatch-late"
			add_run release.yaml "" main workflow_dispatch success >/dev/null
			id="$(next_id)"
			echo "$id $ref $version" >"$state/pending"
			echo "$id"
		else
			dispatched "$ref" "$version" ""
		fi
		;;
	*/pulls\?*)
		query="${endpoint#*\?}"
		prstate="$(sed -n 's/.*state=\([a-z]*\).*/\1/p' <<<"$query")"
		head="$(sed -n 's/.*head=\([^&]*\).*/\1/p' <<<"$query")"
		pulls "$prstate" "$head" "$jq"
		;;
	*) exit 2 ;;
	esac
	exit 0
fi
group="$1 $2"
shift 2
args=" $* "
positional=""
workflow="" commit="" branch="" event=""
while (($#)); do
	case "$1" in
	--workflow) workflow="$2" ;;
	--commit) commit="$2" ;;
	--branch) branch="$2" ;;
	--event) event="$2" ;;
	--repo | --json | --jq | --limit | --interval) ;;
	--*)
		shift
		continue
		;;
	*)
		[[ -n "$positional" ]] || positional="$1"
		shift
		continue
		;;
	esac
	shift 2
done
case "$group" in
"release view")
	[[ -f "$state/release" ]]
	cat "$state/release"
	;;
"run list")
	for id in $(ls "$state/runs" | sort -rn); do
		IFS='|' read -r w c b e _ <"$state/runs/$id"
		[[ "$w" == "$workflow" && (-z "$commit" || "$c" == "$commit") && (-z "$branch" || "$b" == "$branch") && (-z "$event" || "$e" == "$event") ]] || continue
		line "$id"
		break
	done
	;;
"run view")
	if [[ -f "$state/pending" ]]; then
		read -r pending_id pending_ref pending_version <"$state/pending"
		if [[ "$positional" == "$pending_id" ]]; then
			if [[ ! -f "$state/pending-seen" ]]; then
				touch "$state/pending-seen"
				exit 1
			fi
			rm "$state/pending" "$state/pending-seen"
			dispatched "$pending_ref" "$pending_version" "$pending_id" >/dev/null
		fi
	fi
	line "$positional"
	;;
"run watch") ;;
"run rerun")
	echo "run rerun $positional" >>"$state/log"
	IFS='|' read -r w c b e _ _ created url <"$state/runs/$positional"
	result="$(knob rerun-result success)"
	echo "$w|$c|$b|$e|completed|$result|$created|$url" >"$state/runs/$positional"
	if [[ "$w" == release-publish.yaml && "$result" == success ]]; then echo false >"$state/release"; fi
	;;
"pr diff") echo "+version = \"0.99.1\"" ;;
"pr review") echo "pr review $positional" >>"$state/log" ;;
"pr merge")
	[[ "$args" == *" --admin "* && "$args" == *" --match-head-commit head"* ]] || { echo "pr merge without --admin --match-head-commit" >&2; exit 1; }
	echo "pr merge $positional" >>"$state/log"
	read -r _ _ url <"$state/pr-open"
	rm "$state/pr-open"
	echo "merge1 $url" >"$state/pr-merged"
	mv "$state/pr-open-branch" "$state/pr-merged-branch"
	mv "$state/pr-open-author" "$state/pr-merged-author"
	result="$(knob publish-result success)"
	add_run release-publish.yaml merge1 main push "$result" >/dev/null
	if [[ "$result" == success ]]; then
		echo false >"$state/release"
	elif [[ -f "$state/publish-leaves-draft" ]]; then
		echo true >"$state/release"
	fi
	;;
*) exit 2 ;;
esac
"#;

/// Stands in for scripts/flathub-release.sh: logs its arguments.
const FAKE_FLATHUB: &str = r#"#!/usr/bin/env bash
echo "flathub $*" >>"${GH_STATE:?}/log"
"#;

struct Setup {
    gx52: Scratch,
    outside: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let gx52 = Scratch::new();
        gx52.copy_script(SCRIPT);
        let outside = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary dir: {error}"));
        let setup = Self { gx52, outside };
        fs::create_dir_all(setup.path("state")).unwrap_or_else(|error| panic!("state: {error}"));
        for (name, content) in [("gh", FAKE_GH), ("flathub", FAKE_FLATHUB)] {
            let path = setup.path(name);
            fs::write(&path, content).unwrap_or_else(|error| panic!("write {name}: {error}"));
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .unwrap_or_else(|error| panic!("chmod {name}: {error}"));
        }
        setup
    }

    fn path(&self, relative: &str) -> String {
        self.outside.path().join(relative).display().to_string()
    }

    fn knob(&self, name: &str, value: &str) {
        fs::write(self.path(&format!("state/{name}")), value)
            .unwrap_or_else(|error| panic!("knob {name}: {error}"));
    }

    fn run(&self, args: &[&str], yes: bool) -> Run {
        let gh = self.path("gh");
        let flathub = self.path("flathub");
        let state = self.path("state");
        self.gx52.run(
            SCRIPT,
            args,
            &[
                ("GH", gh.as_str()),
                ("FLATHUB_RELEASE", flathub.as_str()),
                ("GH_STATE", state.as_str()),
                ("TRUNK", TRUNK),
                ("POLL_SECONDS", "0"),
                ("ASSUME_YES", if yes { "1" } else { "0" }),
            ],
        )
    }

    fn succeeds(&self, args: &[&str]) -> String {
        let run = self.run(args, true);
        assert!(run.success, "{args:?} failed:\n{}", run.text);
        run.text
    }

    fn stops(&self, args: &[&str], yes: bool, reason: &str) {
        let run = self.run(args, yes);
        assert!(
            !run.success,
            "{args:?} went through, want it to stop:\n{}",
            run.text
        );
        assert!(
            run.text.contains(reason),
            "{args:?}: want '{reason}', got:\n{}",
            run.text
        );
    }

    fn log(&self) -> Vec<String> {
        fs::read_to_string(self.path("state/log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn a_release_runs_from_the_dispatch_to_the_flathub_pull_request() {
    let setup = Setup::new();
    let output = setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 0.99.1",
            "pr review 28",
            "pr merge 28",
            "flathub pr 0.99.1 beta",
        ]
    );
    assert!(output.contains("The release v0.99.1 exists"), "{output}");
    assert!(
        output.contains("make flathub-merge FLATHUB_BASE=beta"),
        "{output}"
    );

    // Done is done: running it again only refreshes the Flathub pull request.
    setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup.log().last().map(String::as_str),
        Some("flathub pr 0.99.1 beta")
    );
    assert_eq!(
        setup.log().len(),
        5,
        "a second run did more than the Flathub step"
    );
}

#[test]
fn a_declined_merge_is_resumed_without_a_second_dispatch() {
    let setup = Setup::new();
    setup.stops(&["0.99.1"], false, "not merged: run make release again");
    assert_eq!(setup.log(), ["workflow run release.yaml main 0.99.1"]);
    setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 0.99.1",
            "pr review 28",
            "pr merge 28",
            "flathub pr 0.99.1 beta",
        ]
    );
}

#[test]
fn a_failed_publish_is_re_run_once_and_resumed_after_the_fix() {
    let setup = Setup::new();
    setup.knob("publish-result", "failure");
    setup.knob("rerun-result", "failure");
    setup.stops(
        &["0.99.1"],
        true,
        "release-publish.yaml failed again after a re-run",
    );
    // The cause is fixed: the next re-run passes. The resumed run neither dispatches nor merges.
    setup.knob("rerun-result", "success");
    setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 0.99.1",
            "pr review 28",
            "pr merge 28",
            "run rerun 3",
            "run rerun 3",
            "flathub pr 0.99.1 beta",
        ]
    );
}

#[test]
fn a_draft_left_by_a_failed_publish_is_finished_not_skipped() {
    let setup = Setup::new();
    setup.knob("publish-result", "failure");
    setup.knob("publish-leaves-draft", "");
    setup.knob("rerun-result", "failure");
    setup.stops(
        &["0.99.1"],
        true,
        "release-publish.yaml failed again after a re-run",
    );
    // A draft exists now; it is not the release, so the resumed run re-runs the publish before
    // anything reaches Flathub.
    setup.knob("rerun-result", "success");
    setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 0.99.1",
            "pr review 28",
            "pr merge 28",
            "run rerun 3",
            "run rerun 3",
            "flathub pr 0.99.1 beta",
        ]
    );
}

#[test]
fn a_bump_behind_the_fixed_trunk_is_refreshed() {
    let setup = Setup::new();
    setup.stops(&["0.99.1"], false, "not merged: run make release again");
    // Fixed on the trunk, which moved on: the bump pull request is refreshed.
    setup.knob("behind", "1");
    let output = setup.succeeds(&["0.99.1"]);
    assert!(output.contains("is 1 commit(s) behind main"), "{output}");
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 0.99.1",
            "workflow run release.yaml main 0.99.1",
            "pr review 28",
            "pr merge 28",
            "flathub pr 0.99.1 beta",
        ]
    );
}

#[test]
fn a_failed_release_workflow_is_dispatched_again() {
    let setup = Setup::new();
    setup.knob("dispatch-result", "failure");
    setup.stops(&["0.99.1"], false, "the release workflow failed");
    setup.knob("dispatch-result", "success");
    setup.succeeds(&["0.99.1"]);
    assert_eq!(
        setup
            .log()
            .iter()
            .filter(|line| line.starts_with("workflow run release.yaml"))
            .count(),
        2
    );
}

#[test]
fn flathub_branches_follow_the_version() {
    let setup = Setup::new();
    setup.stops(
        &["0.99.1", "master"],
        true,
        "a 0.x release: it goes to beta only",
    );
    setup.stops(&["0.99.1", "stable"], true, "beta or master (got 'stable')");
    setup.stops(&["0.99"], true, "must be X.Y.Z");
    // Any blank separates the branches, so a tab does not hide master from the 0.x check.
    setup.stops(&["0.99.1", "beta\tmaster"], true, "a 0.x release");
    setup.stops(&["0.99.1", "beta\nmaster"], true, "a 0.x release");
    assert!(setup.log().is_empty(), "a refused run called GitHub");

    // From 1.0.0 on, straight to stable, or to both (here with the release already published).
    let both = Setup::new();
    both.knob("release", "false");
    both.succeeds(&["1.0.0", "beta master"]);
    assert_eq!(
        both.log(),
        ["flathub pr 1.0.0 beta", "flathub pr 1.0.0 master"]
    );
}

#[test]
fn an_empty_version_is_the_one_the_release_workflow_derives() {
    let setup = Setup::new();
    let output = setup.succeeds(&["", "beta master"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main -",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
            "flathub pr 1.1.0 master",
        ]
    );
    assert!(
        output.contains("The release workflow derived v1.1.0"),
        "{output}"
    );
    assert!(
        output.contains("make release VERSION=1.1.0 FLATHUB_BASE='beta master'"),
        "{output}"
    );

    // No argument at all is the same as an empty one.
    let bare = Setup::new();
    bare.succeeds(&[]);
    assert_eq!(
        bare.log().first().map(String::as_str),
        Some("workflow run release.yaml main -")
    );
}

#[test]
fn an_empty_version_resumes_the_open_bump_pull_request() {
    let setup = Setup::new();
    setup.stops(&[""], false, "not merged: run make release again");
    setup.succeeds(&[""]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main -",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
        ]
    );
}

#[test]
fn an_empty_version_resumes_a_merged_bump_whose_release_is_missing() {
    let setup = Setup::new();
    setup.knob("publish-result", "failure");
    setup.knob("rerun-result", "failure");
    setup.stops(
        &[""],
        true,
        "release-publish.yaml failed again after a re-run",
    );
    setup.knob("rerun-result", "success");
    let output = setup.succeeds(&[""]);
    assert!(output.contains("Resuming v1.1.0"), "{output}");
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main -",
            "pr review 28",
            "pr merge 28",
            "run rerun 3",
            "run rerun 3",
            "flathub pr 1.1.0 beta",
        ]
    );
}

#[test]
fn a_derived_release_is_refreshed_with_its_version() {
    let setup = Setup::new();
    setup.stops(&[""], false, "not merged: run make release again");
    // The trunk moved on and gained a feat: the refresh keeps the version the bump branch carries.
    setup.knob("behind", "1");
    setup.knob("derived", "1.2.0");
    setup.succeeds(&[""]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main -",
            "workflow run release.yaml main 1.1.0",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
        ]
    );
}

#[test]
fn a_derived_0_x_release_does_not_reach_master() {
    let setup = Setup::new();
    setup.knob("derived", "0.99.2");
    setup.stops(&["", "master"], true, "v0.99.2 is a 0.x release");
    assert_eq!(setup.log(), ["workflow run release.yaml main -"]);
}

#[test]
fn a_bump_pull_request_a_person_opened_is_never_resumed() {
    let setup = Setup::new();
    // An open release/v1.1.0 pull request from the same repository, opened by a person.
    setup.knob(
        "pr-open",
        "29 human https://github.com/leinardi/gx52/pull/29",
    );
    setup.knob("pr-open-branch", "release/v1.1.0");
    setup.knob("pr-open-author", "someone");
    setup.succeeds(&[""]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main -",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
        ]
    );
}

#[test]
fn another_run_does_not_stand_in_for_the_dispatch() {
    let setup = Setup::new();
    // A dry run on the trunk shows up right after the dispatch, before the dispatch's own run.
    setup.knob("dispatch-late", "");
    setup.succeeds(&["1.1.0"]);
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 1.1.0",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
        ]
    );
}

#[test]
fn the_bump_ci_is_shown_not_waited_for() {
    let setup = Setup::new();
    // release-publish.yaml gates the merge commit: a failed bump CI neither stops nor is re-run.
    setup.knob("ci-result", "failure");
    let output = setup.succeeds(&["1.1.0"]);
    assert!(
        output.contains("CI on release/v1.1.0: completed, failure (not waited for)"),
        "{output}"
    );
    // The prompt names the pull request, not the CI run.
    assert!(
        output.contains("Approve and merge https://github.com/leinardi/gx52/pull/28"),
        "{output}"
    );
    assert_eq!(
        setup.log(),
        [
            "workflow run release.yaml main 1.1.0",
            "pr review 28",
            "pr merge 28",
            "flathub pr 1.1.0 beta",
        ]
    );
}
