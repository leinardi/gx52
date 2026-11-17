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
#      which refreshes it; otherwise wait for its CI (dispatched again if missing; a failed run is
#      offered a re-run), show the diff, and approve and merge it on confirmation;
#   4. nothing yet: dispatch release.yaml and wait for it.
#
# Then scripts/flathub-release.sh opens (or updates) the Flathub pull request into each of BASES:
# beta, master, or "beta master". master needs 1.0.0 or later: a 0.x release goes to beta only.
#
# With --dry-run instead of BASES it dispatches the dry run of VERSION on the current branch (as
# pushed) and waits for it, and does nothing else.
#
# Usage: scripts/release.sh VERSION [BASES | --dry-run]   (BASES: default beta)
#
# Environment: ASSUME_YES=1 answers every prompt yes and ASSUME_YES=0 no (otherwise they are read
# from the terminal, and no terminal is a no). For the tests: GH (default gh), FLATHUB_RELEASE (default
# scripts/flathub-release.sh), REPOSITORY (default leinardi/gx52), TRUNK (default rust-rewrite)
# and POLL_SECONDS (default 15).

set -euo pipefail

die() {
	echo "release: $*" >&2
	exit 1
}

GH="${GH:-gh}"
REPOSITORY="${REPOSITORY:-leinardi/gx52}"
TRUNK="${TRUNK:-rust-rewrite}"
POLL_SECONDS="${POLL_SECONDS:-15}"
FLATHUB_RELEASE="${FLATHUB_RELEASE:-$(dirname "${BASH_SOURCE[0]}")/flathub-release.sh}"

[[ $# -ge 1 && $# -le 2 ]] || die "usage: release.sh VERSION [BASES | --dry-run]"
VERSION="$1"
BASES="${2:-beta}"
DRY_RUN=false
if [[ "${BASES}" == --dry-run ]]; then
	DRY_RUN=true
	BASES=beta
fi
[[ "${VERSION}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
	die "the version must be X.Y.Z (got '${VERSION}')"
TAG="v${VERSION}"
BRANCH="release/${TAG}"
[[ -n "${BASES// /}" ]] || die "no Flathub branch given"
for base in ${BASES}; do
	case "${base}" in
	beta) ;;
	master) [[ "${VERSION}" != 0.* ]] || die "${TAG} is a 0.x release: it goes to beta only" ;;
	*) die "a Flathub branch is beta or master (got '${base}')" ;;
	esac
done
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

# Waits for the newest run of workflow $2 matching the filters after it; a run created before $1
# (an ISO time, empty: any) does not count. Prints its line once it exists (after 5 minutes: dies).
await_run() {
	local since="$1" workflow="$2"
	shift 2
	local run tries=0
	while :; do
		run="$(latest_run "${workflow}" "$@")"
		if [[ -n "${run}" ]] && [[ -z "${since}" || ! "$(cut -d' ' -f4 <<<"${run}")" < "${since}" ]]; then
			echo "${run}"
			return
		fi
		((tries += 1))
		((tries * POLL_SECONDS < 300)) || die "no ${workflow} run appeared on GitHub after 5 minutes"
		sleep "${POLL_SECONDS}"
	done
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

# "merge-commit url" of the merged bump pull request, empty when there is none.
merged_bump() {
	"${GH}" pr list --repo "${REPOSITORY}" --head "${BRANCH}" --base "${TRUNK}" --state merged \
		--json mergeCommit,url --jq '.[0] | select(.) | "\(.mergeCommit.oid) \(.url)"'
}

# "number head-commit url" of the open bump pull request, empty when there is none.
open_bump() {
	"${GH}" pr list --repo "${REPOSITORY}" --head "${BRANCH}" --base "${TRUNK}" --state open \
		--json number,headRefOid,url --jq '.[0] | select(.) | "\(.number) \(.headRefOid) \(.url)"'
}

# A minute ago, so a run GitHub stamps with a clock slightly behind this one still counts.
now() {
	date -u -d '-1 minute' +%Y-%m-%dT%H:%M:%SZ
}

dispatch_release() {
	local since
	since="$(now)"
	echo "Dispatching the release of ${TAG} on ${TRUNK}."
	"${GH}" workflow run release.yaml --repo "${REPOSITORY}" --ref "${TRUNK}" \
		-f version="${VERSION}" -f dry_run=false
	see_through "$(await_run "${since}" release.yaml --event workflow_dispatch --branch "${TRUNK}")" \
		"the release workflow"
}

publish() {
	local merge="$1"
	see_through "$(await_run "" release-publish.yaml --commit "${merge}")" "release-publish.yaml"
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
	local ci
	ci="$(latest_run ci.yaml --commit "${head}" --event workflow_dispatch)"
	if [[ -z "${ci}" ]]; then
		local since
		since="$(now)"
		echo "No CI run on ${BRANCH}: dispatching it."
		"${GH}" workflow run ci.yaml --repo "${REPOSITORY}" --ref "${BRANCH}"
		ci="$(await_run "${since}" ci.yaml --commit "${head}" --event workflow_dispatch)"
	fi
	see_through "${ci}" "CI on ${BRANCH}"
	"${GH}" pr diff "${number}" --repo "${REPOSITORY}"
	confirm "Approve and merge ${url} (a merge commit), releasing ${TAG}?" ||
		die "not merged: run make release again to merge it"
	"${GH}" pr review "${number}" --repo "${REPOSITORY}" --approve
	"${GH}" pr merge "${number}" --repo "${REPOSITORY}" --merge
}

if [[ "${DRY_RUN}" == true ]]; then
	ref="$(git rev-parse --abbrev-ref HEAD)"
	git ls-remote --exit-code --heads origin "${ref}" >/dev/null ||
		die "${ref} is not pushed: a dry run runs the branch as GitHub has it"
	since="$(now)"
	echo "Dispatching the dry run of ${TAG} on ${ref}."
	"${GH}" workflow run release.yaml --repo "${REPOSITORY}" --ref "${ref}" -f version="${VERSION}"
	see_through "$(await_run "${since}" release.yaml --event workflow_dispatch --branch "${ref}")" \
		"the dry run"
	exit 0
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

for base in ${BASES}; do
	echo
	echo "Flathub ${base}:"
	"${FLATHUB_RELEASE}" pr "${VERSION}" "${base}"
done
echo
for base in ${BASES}; do
	echo "Once Flathub's bot built the ${base} pull request: make flathub-merge FLATHUB_BASE=${base}"
done
