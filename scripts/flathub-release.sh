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
# The maintainer's side of the Flathub repository (docs/release.md, "Flathub by hand"). It keeps a
# clone of flathub/com.leinardi.gx52 under FLATHUB_DIR, prepares a branch there, pushes it with the
# maintainer's own git login and opens its pull request with the maintainer's gh login (or updates
# the one already open). It never pushes to `master`, and pushes `beta` only to create it.
#
#   pr VERSION BASE   a branch gx52/vVERSION-BASE from BASE (beta or master) whose tree is the
#                     release vVERSION: its com.leinardi.gx52.json and cargo-sources.json assets,
#                     build-aux/flathub/flathub.json and update.yml, BASE's README.md, and nothing
#                     else. Against master before 1.0.0 it is the test build of the beta branch
#                     (never merged); from 1.0.0 on, the stable migration.
#   beta VERSION      creates `beta` from master with gx52/vVERSION-master merged in, once the
#                     bot's test build of that pull request passed, and closes that pull request.
#   workflow          a branch gx52/update-workflow from master adding .github/workflows/update.yml,
#                     which only runs from the default branch.
#
# The assets are downloaded from the release, and the manifest must name the tag and the commit
# the tag points at. Re-running a command rebuilds its branch from the base and force-pushes it
# (with a lease), so it also refreshes an open pull request.
#
# Usage: scripts/flathub-release.sh pr VERSION BASE | beta VERSION | workflow
#
# Environment: FLATHUB_DIR (the directory holding the Flathub clone, required), and for the tests
# FLATHUB_URL, UPSTREAM_URL, RELEASES_URL (default: the GitHub repositories and their release
# downloads) and GH (default: gh).

set -euo pipefail

die() {
	echo "flathub-release: $*" >&2
	exit 1
}

APP_ID=com.leinardi.gx52
FLATHUB_REPO="flathub/${APP_ID}"
FLATHUB_URL="${FLATHUB_URL:-https://github.com/${FLATHUB_REPO}.git}"
UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/leinardi/gx52.git}"
RELEASE_PAGES=https://github.com/leinardi/gx52/releases
RELEASES_URL="${RELEASES_URL:-${RELEASE_PAGES}}"
GH="${GH:-gh}"
FILES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../build-aux/flathub" && pwd)"
APP_SOURCE='.modules[] | select(type == "object" and .name == "gx52") | .sources[]
	| select(type == "object" and .type == "git")'

usage() {
	die "usage: flathub-release.sh pr VERSION BASE | beta VERSION | workflow"
}

check_version() {
	[[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
		die "the version must be X.Y.Z (got '$1')"
}

# The clone, fetched and clean, in the working directory.
open_clone() {
	[[ -n "${FLATHUB_DIR:-}" ]] || die "FLATHUB_DIR is not set"
	local clone="${FLATHUB_DIR}/${APP_ID}"
	if [[ ! -d "${clone}/.git" ]]; then
		mkdir -p "${FLATHUB_DIR}"
		git clone --quiet "${FLATHUB_URL}" "${clone}"
	fi
	cd "${clone}"
	local origin
	origin="$(git remote get-url origin)"
	[[ "${origin}" == "${FLATHUB_URL}" ]] || die "${clone} is a clone of ${origin}, not ${FLATHUB_URL}"
	[[ -z "$(git status --porcelain)" ]] || die "${clone} has uncommitted changes"
	git fetch --quiet --prune origin
}

has_branch() {
	git show-ref --verify --quiet "refs/remotes/origin/$1"
}

# The commit tag $1 names on the upstream repository (the peeled one for an annotated tag).
tag_commit() {
	local refs
	refs="$(git ls-remote "${UPSTREAM_URL}" "refs/tags/$1" "refs/tags/$1^{}")"
	[[ -n "${refs}" ]] || die "the tag $1 does not exist on ${UPSTREAM_URL}"
	awk -v peeled="refs/tags/$1^{}" '
		$2 == peeled { found = $1 } { last = $1 } END { print (found != "" ? found : last) }' <<<"${refs}"
}

# Downloads asset $2 of release $1 into directory $3.
download() {
	curl -fsSL --retry 3 -o "$3/$2" "${RELEASES_URL}/download/$1/$2" ||
		die "could not download $2 of the release $1"
}

# Rebuilds branch $1 from origin/$2, lets the caller change the tree, commits it with message $3,
# and pushes it. Exits 0 with a note when the tree equals the base's.
commit_and_push() {
	local branch="$1" base="$2" message="$3"
	git add --all
	if git diff --cached --quiet "origin/${base}"; then
		echo "Nothing to change: ${base} already has this tree."
		git switch --quiet --detach "origin/${base}"
		git branch --quiet -D "${branch}"
		exit 0
	fi
	git commit --quiet --no-verify -m "${message}"
	git --no-pager show --stat --format='%s' HEAD
	git push --quiet --force-with-lease origin "HEAD:refs/heads/${branch}"
}

# The URL of the open pull request of branch $1 into $2, empty when there is none.
open_pull_request() {
	"${GH}" pr list --repo "${FLATHUB_REPO}" --head "$1" --base "$2" --state open \
		--json url --jq '.[0].url // empty'
}

# Opens the pull request of branch $1 into $2 with title $3 and body $4, or updates the open one.
publish_pull_request() {
	local url
	url="$(open_pull_request "$1" "$2")"
	if [[ -n "${url}" ]]; then
		"${GH}" pr edit "${url}" --repo "${FLATHUB_REPO}" --title "$3" --body "$4" >/dev/null
		echo
		echo "Updated the pull request: ${url}"
	else
		url="$("${GH}" pr create --repo "${FLATHUB_REPO}" --base "$2" --head "$1" --title "$3" --body "$4")"
		echo
		echo "Opened the pull request: ${url}"
	fi
}

pr() {
	local version="$1" base="$2"
	check_version "${version}"
	[[ "${base}" == beta || "${base}" == master ]] || die "the base must be beta or master (got '${base}')"
	local tag="v${version}" branch="gx52/v${version}-${base}"
	local commit
	commit="$(tag_commit "${tag}")"

	# Fetched and checked before the clone is touched, so a refused release leaves it as it was.
	ASSETS="$(mktemp -d)"
	trap 'rm -rf "${ASSETS}"' EXIT
	download "${tag}" "${APP_ID}.json" "${ASSETS}"
	download "${tag}" cargo-sources.json "${ASSETS}"
	jq -e --arg tag "${tag}" --arg commit "${commit}" \
		"[${APP_SOURCE}] | length == 1 and .[0].tag == \$tag and .[0].commit == \$commit" \
		"${ASSETS}/${APP_ID}.json" >/dev/null ||
		die "the release's ${APP_ID}.json does not name ${tag} at ${commit}"
	jq -e 'type == "array" and length > 0' "${ASSETS}/cargo-sources.json" >/dev/null ||
		die "the release's cargo-sources.json is not a list of sources"

	open_clone
	has_branch "${base}" || die "${FLATHUB_REPO} has no ${base} branch yet: run 'pr ${version} master' and 'beta ${version}' first"
	local legacy=true
	if git show "origin/${base}:${APP_ID}.json" 2>/dev/null | jq -e '[.. | objects | select(has("x-checker-data"))] | length > 0' >/dev/null 2>&1; then
		legacy=false
	fi

	git switch --quiet --force-create "${branch}" "origin/${base}"
	git rm -r --quiet -- .
	if git cat-file -e "origin/${base}:README.md" 2>/dev/null; then
		git checkout "origin/${base}" -- README.md
	fi
	cp "${ASSETS}/${APP_ID}.json" "${ASSETS}/cargo-sources.json" .
	cp "${FILES_DIR}/flathub.json" flathub.json
	mkdir -p .github/workflows
	cp "${FILES_DIR}/update.yml" .github/workflows/update.yml

	local title body
	if [[ "${legacy}" == true ]]; then
		title="Migrate to the Rust rewrite (${tag})"
	else
		title="Update to ${tag}"
	fi
	body="Release: ${RELEASE_PAGES}/tag/${tag}

The manifest and cargo-sources.json are the release's assets; flathub.json and .github/workflows/update.yml come from build-aux/flathub in leinardi/gx52."
	if [[ "${legacy}" == true ]]; then
		body="${body}

Replaces the Python application (0.7.x) with the Rust rewrite and removes the files only it used (the Python modules, pypi-dependencies.json and the submodules)."
	fi
	if [[ "${base}" == master && "${version}" == 0.* ]]; then
		title="${title}: test build for the beta branch"
		body="${body}

Test build for the new beta branch: not to be merged into master. Once the build passes, beta is created from master with this branch merged in, and this pull request is closed."
	fi
	commit_and_push "${branch}" "${base}" "${title}"
	publish_pull_request "${branch}" "${base}" "${title}" "${body}"
	if [[ "${base}" == master && "${version}" == 0.* ]]; then
		echo
		echo "Once the bot's test build passes: make flathub-beta VERSION=${version}"
	fi
}

beta() {
	local version="$1"
	check_version "${version}"
	local source="gx52/v${version}-master"
	open_clone
	! has_branch beta || die "${FLATHUB_REPO} already has a beta branch: update it with 'pr VERSION beta'"
	has_branch "${source}" || die "no ${source} branch: run 'pr ${version} master' first"
	git merge-base --is-ancestor origin/master "origin/${source}" ||
		die "${source} is not based on the current master: run 'pr ${version} master' again"
	git switch --quiet --force-create beta "origin/${source}"
	git push --quiet origin "HEAD:refs/heads/beta"
	local head url
	head="$(git rev-parse --short HEAD)"
	echo "Created beta at ${head}: its official build starts now."
	url="$(open_pull_request "${source}" master)"
	if [[ -n "${url}" ]]; then
		"${GH}" pr close "${url}" --repo "${FLATHUB_REPO}" \
			--comment "beta was created from this branch at ${head}; this test build is not merged into master."
		echo "Closed the test-build pull request: ${url}"
	fi
}

workflow() {
	local branch=gx52/update-workflow
	open_clone
	git switch --quiet --force-create "${branch}" origin/master
	mkdir -p .github/workflows
	cp "${FILES_DIR}/update.yml" .github/workflows/update.yml
	local title="Add the update workflow"
	local body="Adds .github/workflows/update.yml (from build-aux/flathub in leinardi/gx52). It runs flatpak-external-data-checker on the beta and master branches, weekly and on dispatch, and opens one pull request per branch with this repository's GITHUB_TOKEN when a new leinardi/gx52 release tag is out, with that release's cargo-sources.json. A branch whose manifest has no x-checker-data is skipped, so master is untouched until it is migrated.

Scheduled workflows only run from the default branch, hence master."
	commit_and_push "${branch}" master "${title}"
	publish_pull_request "${branch}" master "${title}" "${body}"
}

command -v jq >/dev/null || die "jq is not on PATH"
command -v curl >/dev/null || die "curl is not on PATH"
command -v "${GH}" >/dev/null || die "${GH} is not on PATH"
case "${1:-}" in
pr)
	[[ $# -eq 3 ]] || usage
	pr "$2" "$3"
	;;
beta)
	[[ $# -eq 2 ]] || usage
	beta "$2"
	;;
workflow)
	[[ $# -eq 1 ]] || usage
	workflow
	;;
*) usage ;;
esac
