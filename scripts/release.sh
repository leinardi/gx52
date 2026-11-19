#!/usr/bin/env bash
#
# Copyright 2026 Roberto Leinardi.
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# `make release` (docs/release.md, "How to release"): drives one release from the dispatch to the
# Flathub pull request, with the maintainer's gh login, and stops at the one decision that is
# theirs (approving and merging the bump pull request). Every step is read back from GitHub before
# it is taken, so a run that stopped (a failed job, a declined prompt, a closed terminal) is resumed
# by running the same command again, once the cause is fixed:
#
#   1. the release vVERSION exists: nothing left here, on to Flathub;
#   2. the bump pull request is merged: wait for release-publish.yaml on its merge commit; a failed
#      or cancelled run is offered a re-run;
#   3. the bump pull request is open: if the trunk moved on since, dispatch the release again,
#      which refreshes it; otherwise show the diff, and approve and merge it on confirmation. Its
#      CI is not waited for (a run that needs approval may never start): release-publish.yaml
#      waits for CI on the merge commit before it tags anything;
#   4. nothing yet: dispatch release.yaml and wait for it.
#
# Then scripts/flathub-release.sh opens (or updates) the Flathub pull request into each of BASES:
# beta, master, or "beta master". master needs 1.0.0 or later: a 0.x release goes to beta only.
#
# With an empty VERSION the version is the one the Release workflow derives with svu (allowed from
# 1.0.0 on, scripts/release-policy.sh), read back from GitHub: an open release/vX.Y.Z pull request
# into the trunk is resumed, then a merged one whose release is not published yet; with neither,
# release.yaml is dispatched with no version and the version is read from the pull request it
# opens. Once the release exists, a resumed Flathub step needs VERSION again: an empty one would
# start the next release.
#
# With --dry-run instead of BASES it dispatches the dry run of VERSION (empty: derived) on the
# current branch (as pushed) and waits for it, and does nothing else.
#
# Usage: scripts/release.sh [VERSION] [BASES | --dry-run]   (VERSION: default derived; BASES:
# default beta)
#
# Environment: ASSUME_YES=1 answers every prompt yes and ASSUME_YES=0 no (otherwise they are read
# from the terminal, and no terminal is a no). For the tests: GH (default gh), FLATHUB_RELEASE (default
# scripts/flathub-release.sh), REPOSITORY (default leinardi/gx52), TRUNK (default main)
# and POLL_SECONDS (default 15).

set -euo pipefail

die() {
	echo "release: $*" >&2
	exit 1
}

GH="${GH:-gh}"
REPOSITORY="${REPOSITORY:-leinardi/gx52}"
TRUNK="${TRUNK:-main}"
POLL_SECONDS="${POLL_SECONDS:-15}"
FLATHUB_RELEASE="${FLATHUB_RELEASE:-$(dirname "${BASH_SOURCE[0]}")/flathub-release.sh}"

[[ $# -le 2 ]] || die "usage: release.sh [VERSION] [BASES | --dry-run]"
VERSION="${1:-}"
BASES="${2:-beta}"
DRY_RUN=false
if [[ "${BASES}" == --dry-run ]]; then
	DRY_RUN=true
	BASES=beta
fi
VERSION_REGEX='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
[[ -z "${VERSION}" || "${VERSION}" =~ ${VERSION_REGEX} ]] ||
	die "the version must be X.Y.Z (got '${VERSION}')"
# The Flathub branches, split once on any blank (a newline too): the 0.x check and the Flathub
# step use the same list.
read -r -d '' -a BASE_LIST <<<"${BASES}" || true
((${#BASE_LIST[@]} > 0)) || die "no Flathub branch given"
BASES="${BASE_LIST[*]}"
for base in "${BASE_LIST[@]}"; do
	[[ "${base}" == beta || "${base}" == master ]] || die "a Flathub branch is beta or master (got '${base}')"
done

TAG=""
BRANCH=""
# Sets TAG and BRANCH from VERSION, and checks the Flathub branches against it.
set_version() {
	TAG="v${VERSION}"
	BRANCH="release/${TAG}"
	local base
	[[ "${DRY_RUN}" == false && "${VERSION}" == 0.* ]] || return 0
	for base in "${BASE_LIST[@]}"; do
		[[ "${base}" != master ]] || die "${TAG} is a 0.x release: it goes to beta only"
	done
}
[[ -z "${VERSION}" ]] || set_version
command -v "${GH}" >/dev/null || die "${GH} is not on PATH"

# Asks $1; yes only on an explicit y. ASSUME_YES=1 answers yes and ASSUME_YES=0 no, unasked.
confirm() {
	if [[ "${ASSUME_YES:-}" == 1 ]]; then
		echo "$1 [y/N] y (ASSUME_YES)"
		return 0
	fi
	if [[ "${ASSUME_YES:-}" == 0 ]]; then
		echo "$1 [y/N] n (ASSUME_YES)"
		return 1
	fi
	local answer=""
	if ! { exec 3</dev/tty; } 2>/dev/null; then
		echo "$1 [y/N] n (no terminal)"
		return 1
	fi
	read -r -u 3 -p "$1 [y/N] " answer || true
	exec 3<&-
	[[ "${answer}" == [yY] || "${answer}" == [yY][eE][sS] ]]
}

# A run as one line, "id status conclusion created url" (conclusion "none" while it runs).
RUN_FIELDS=databaseId,status,conclusion,createdAt,url
RUN_LINE='"\(.databaseId) \(.status) \(.conclusion // "none" | if . == "" then "none" else . end) \(.createdAt) \(.url)"'

# The newest run of workflow $1 matching the extra filters, empty when there is none.
latest_run() {
	local workflow="$1"
	shift
	"${GH}" run list --repo "${REPOSITORY}" --workflow "${workflow}" "$@" --limit 1 \
		--json "${RUN_FIELDS}" --jq ".[0] | select(.) | ${RUN_LINE}"
}

run_view() {
	"${GH}" run view "$1" --repo "${REPOSITORY}" --json "${RUN_FIELDS}" --jq "${RUN_LINE}"
}

# Waits for the newest run of workflow $1 matching the filters after it. Prints its line once it
# exists (after 5 minutes: dies).
await_run() {
	local workflow="$1"
	shift
	local run tries=0
	while :; do
		run="$(latest_run "${workflow}" "$@")"
		if [[ -n "${run}" ]]; then
			echo "${run}"
			return
		fi
		((tries += 1))
		((tries * POLL_SECONDS < 300)) || die "no ${workflow} run appeared on GitHub after 5 minutes"
		sleep "${POLL_SECONDS}"
	done
}

# Dispatches workflow $1 on ref $2 with the inputs after them (name=value) and prints the id of the
# run it started, as GitHub returns it (return_run_details): what follows is that exact run, never
# another dispatch of the same workflow (a dry run started meanwhile).
dispatch() {
	local workflow="$1" ref="$2" input id
	shift 2
	local fields=(-f "ref=${ref}" -F return_run_details=true)
	for input in "$@"; do
		fields+=(-f "inputs[${input%%=*}]=${input#*=}")
	done
	id="$("${GH}" api -X POST "repos/${REPOSITORY}/actions/workflows/${workflow}/dispatches" \
		"${fields[@]}" --jq '.workflow_run_id // empty')"
	[[ "${id}" =~ ^[0-9]+$ ]] || die "GitHub returned no run id for the ${workflow} dispatch (got '${id}')"
	echo "${id}"
}

# The line of run $1, once GitHub shows it (after 5 minutes: dies).
await_run_id() {
	local run tries=0
	while ! run="$(run_view "$1" 2>/dev/null)" || [[ -z "${run}" ]]; do
		((tries += 1))
		((tries * POLL_SECONDS < 300)) || die "run $1 did not appear on GitHub after 5 minutes"
		sleep "${POLL_SECONDS}"
	done
	echo "${run}"
}

# Watches run $1 (a latest_run line) to its end; offers one re-run of a failed or cancelled one
# ($2 names it in the messages). Returns once it succeeded; dies when it failed and was not re-run,
# or failed again.
see_through() {
	local run="$1" what="$2" id status conclusion url reruns=0
	while :; do
		read -r id status conclusion _ url <<<"${run}"
		if [[ "${status}" != completed ]]; then
			echo "Waiting for ${what}: ${url}"
			"${GH}" run watch "${id}" --repo "${REPOSITORY}" --interval "${POLL_SECONDS}" >/dev/null 2>&1 || true
			run="$(run_view "${id}")"
			continue
		fi
		if [[ "${conclusion}" == success ]]; then
			echo "${what} passed: ${url}"
			return
		fi
		((reruns == 0)) || die "${what} failed again after a re-run (${url}): fix the cause" \
			"(docs/release.md, \"Recovering a failed run\"), then run make release again"
		case "${conclusion}" in
		cancelled)
			confirm "${what} was cancelled (${url}). Re-run all its jobs?" ||
				die "${what} was cancelled: re-run it, or run make release again once the cause is fixed"
			"${GH}" run rerun "${id}" --repo "${REPOSITORY}"
			((reruns += 1))
			;;
		*)
			confirm "${what} failed (${url}). Re-run its failed jobs (a transient failure)?" ||
				die "${what} failed: fix the cause (docs/release.md, \"Recovering a failed run\"), then run make release again"
			"${GH}" run rerun "${id}" --repo "${REPOSITORY}" --failed
			((reruns += 1))
			;;
		esac
		sleep "${POLL_SECONDS}"
		run="$(run_view "${id}")"
	done
}

# A published release: a draft (release-publish.yaml stopped while publishing) is not one yet,
# and re-running that run finishes it.
release_exists() {
	[[ "$("${GH}" release view "${TAG}" --repo "${REPOSITORY}" --json isDraft --jq .isDraft 2>/dev/null)" == false ]]
}

# The bump pull requests into the trunk in state $1 (open or closed), from branch $2 (empty: any
# release/vX.Y.Z), each printed through the jq expression $3. Only github-actions[bot] opens a bump
# pull request from this repository, and release-on-merge publishes only one it opened: a pull
# request a person opened from release/vX.Y.Z is never resumed or merged. The pulls API is read,
# every page, and filtered here: GitHub's search (gh pr list --author) can lag behind a pull
# request the workflow just opened, and a --limit would cut the list before the filter.
bump_pulls() {
	local state="$1" branch="$2" format="$3" query
	query="state=${state}&base=${TRUNK}&per_page=100"
	[[ -z "${branch}" ]] || query+="&head=${REPOSITORY%%/*}:${branch}"
	"${GH}" api --paginate "repos/${REPOSITORY}/pulls?${query}" --jq ".[] |
		select(.user.login == \"github-actions[bot]\" and .head.repo.full_name == \"${REPOSITORY}\"
			and (.head.ref | test(\"^release/v[0-9]+\\\\.[0-9]+\\\\.[0-9]+$\"))) | ${format}"
}

# "merge-commit url" of the merged bump pull request, empty when there is none.
merged_bump() {
	bump_pulls closed "${BRANCH}" 'select(.merged_at) | "\(.merge_commit_sha) \(.html_url)"' | sed -n 1p
}

# "number head-commit url" of the open bump pull request, empty when there is none.
open_bump() {
	bump_pulls open "${BRANCH}" '"\(.number) \(.head.sha) \(.html_url)"' | sed -n 1p
}

# The version input of a dispatch: none when VERSION is empty, so the workflow derives it.
version_input() {
	[[ -z "${VERSION}" ]] || echo "version=${VERSION}"
}

dispatch_release() {
	local inputs id
	mapfile -t inputs < <(version_input)
	echo "Dispatching the release of ${TAG:-the version svu derives} on ${TRUNK}."
	local run
	id="$(dispatch release.yaml "${TRUNK}" "${inputs[@]}" dry_run=false)"
	run="$(await_run_id "${id}")"
	see_through "${run}" "the release workflow"
}

publish() {
	local merge="$1"
	local run
	run="$(await_run release-publish.yaml --commit "${merge}")"
	see_through "${run}" "release-publish.yaml"
	release_exists || die "release-publish.yaml passed, but there is no release ${TAG}"
}

merge_bump() {
	local number="$1" head="$2" url="$3"
	local behind
	behind="$("${GH}" api "repos/${REPOSITORY}/compare/${TRUNK}...${head}" --jq .behind_by)"
	if [[ "${behind}" != 0 ]]; then
		echo "${url} is ${behind} commit(s) behind ${TRUNK}: refreshing it."
		dispatch_release
		return
	fi
	# Shown, not waited for: release-publish.yaml waits for CI on the merge commit, and gates it.
	local ci status conclusion url
	ci="$(latest_run ci.yaml --commit "${head}" --event workflow_dispatch)"
	if [[ -n "${ci}" ]]; then
		read -r _ status conclusion _ url <<<"${ci}"
		echo "CI on ${BRANCH}: ${status}, ${conclusion} (not waited for): ${url}"
	else
		echo "No CI run on ${BRANCH} (not waited for: release-publish.yaml runs CI on the merge commit)."
	fi
	"${GH}" pr diff "${number}" --repo "${REPOSITORY}"
	confirm "Approve and merge ${url} (a merge commit), releasing ${TAG}?" ||
		die "not merged: run make release again to merge it"
	"${GH}" pr review "${number}" --repo "${REPOSITORY}" --approve
	"${GH}" pr merge "${number}" --repo "${REPOSITORY}" --merge
}

# The versions of the bump pull requests into the trunk, one per line: $1 open, every open one;
# $1 merged, the newest merged one.
bump_versions() {
	if [[ "$1" == open ]]; then
		bump_pulls open "" '.head.ref | ltrimstr("release/v")'
	else
		bump_pulls closed "" 'select(.merged_at) | "\(.merged_at) \(.head.ref | ltrimstr("release/v"))"' |
			sort | sed -n '$s/^[^ ]* //p'
	fi
}

# An empty VERSION: the release in progress on GitHub, or the one release.yaml derives.
resolve_version() {
	local open merged
	open="$(bump_versions open)"
	if [[ "$(wc -w <<<"${open}")" -gt 1 ]]; then
		die "more than one bump pull request is open ($(tr '\n' ' ' <<<"${open}")): give VERSION"
	fi
	if [[ -n "${open}" ]]; then
		VERSION="${open}"
		echo "Resuming the open bump pull request of v${VERSION}; a refresh keeps that version" \
			"(give VERSION for another one, after closing it)."
		return
	fi
	merged="$(bump_versions merged)"
	if [[ -n "${merged}" ]]; then
		VERSION="${merged}"
		set_version
		if ! release_exists; then
			echo "Resuming v${VERSION}: its bump pull request is merged, its release is not published."
			return
		fi
		VERSION=""
		TAG=""
	fi
	dispatch_release
	open="$(bump_versions open)"
	[[ "$(wc -w <<<"${open}")" -eq 1 ]] ||
		die "the release workflow passed, but there is not one open bump pull request (${open:-none})"
	VERSION="${open}"
	echo "The release workflow derived v${VERSION}."
}

if [[ "${DRY_RUN}" == true ]]; then
	ref="$(git rev-parse --abbrev-ref HEAD)"
	git ls-remote --exit-code --heads origin "${ref}" >/dev/null ||
		die "${ref} is not pushed: a dry run runs the branch as GitHub has it"
	mapfile -t inputs < <(version_input)
	echo "Dispatching the dry run of ${TAG:-the version svu derives} on ${ref}."
	id="$(dispatch release.yaml "${ref}" "${inputs[@]}")"
	run="$(await_run_id "${id}")"
	see_through "${run}" "the dry run"
	exit 0
fi

if [[ -z "${VERSION}" ]]; then
	resolve_version
	[[ "${VERSION}" =~ ${VERSION_REGEX} ]] || die "GitHub gave the version '${VERSION}', not X.Y.Z"
	set_version
fi

# At most a dispatch, a refresh, a merge and the publish: more means something keeps undoing it.
for _ in 1 2 3 4 5 6; do
	if release_exists; then
		echo "The release ${TAG} exists: https://github.com/${REPOSITORY}/releases/tag/${TAG}"
		break
	fi
	merged="$(merged_bump)"
	if [[ -n "${merged}" ]]; then
		publish "${merged%% *}"
		continue
	fi
	bump="$(open_bump)"
	if [[ -n "${bump}" ]]; then
		read -r number head url <<<"${bump}"
		merge_bump "${number}" "${head}" "${url}"
		continue
	fi
	dispatch_release
	[[ -n "$(open_bump)" ]] || die "the release workflow passed, but opened no pull request from ${BRANCH}"
done
release_exists || die "the release ${TAG} is still missing: run make release again"
echo "A Flathub step that fails is resumed with: make release VERSION=${VERSION} FLATHUB_BASE='${BASES}'"

for base in "${BASE_LIST[@]}"; do
	echo
	echo "Flathub ${base}:"
	"${FLATHUB_RELEASE}" pr "${VERSION}" "${base}"
done
echo
for base in "${BASE_LIST[@]}"; do
	echo "Once Flathub's bot built the ${base} pull request: make flathub-merge FLATHUB_BASE=${base}"
done
