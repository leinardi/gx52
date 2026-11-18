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
# Writes a release version into the four files that carry it (docs/release.md, "One version
# source"): the root package's `[package] version` in Cargo.toml, its Cargo.lock entry, the
# `version:` of meson.build's project() and a new top `<release>` of the metainfo. Called by the
# Release workflow (gh-reusable-workflows' version-bump-pr) as
# `make release-bump VERSION="$RELEASE_VERSION"`, or by hand on a scratch clone; it never commits.
#
# Usage: scripts/release-bump.sh X.Y.Z
#
# The version must be strict X.Y.Z, higher than every release (the `vX.Y.Z` tags and the legacy
# `0.7.x` tags without `v`, never below 0.7.6) and higher than the version Cargo.toml carries. Who
# chose it (explicit, or svu after 1.0.0) is not visible here: scripts/release-policy.sh enforces
# that rule before the version is resolved.
#
# The new `<release>` describes the `feat` and `fix` subjects since the newest `vX.Y.Z` tag; the
# first release of the rewrite (no `v` tag yet) gets a fixed description of the rewrite instead,
# and the first stable one (the first of major 1 or higher, which replaces 0.7.x on Flathub stable)
# that description plus its two notes for 0.7 users, the source-install profiles and the untested
# X52 (non-Pro). A `<release>` of
# the rewrite whose `vX.Y.Z` tag does not exist (its bump was merged, its gate failed, it was never
# published) is removed; the legacy `0.7.x` entries always stay. The date is today in UTC, or the
# day of SOURCE_DATE_EPOCH when that is set.
#
# It refuses a tree with any uncommitted change to a tracked file, and every check on the inputs
# runs before the first write. After writing it fails when anything but the four files changed, or
# when Cargo.lock changed more than the root package's version line, and then restores the tree.
#
# Adapted from rust-common/scripts/release-bump.sh at dbeffe1, originally MIT-licensed,
# Copyright (c) 2026 Roberto Leinardi.

set -euo pipefail

die() {
	echo "release-bump: $*" >&2
	exit 1
}

# Set once the files are written: from then on any failure, a refusal or an unexpected error,
# restores every tracked file (the tree was clean before the write), so a failed bump never leaves
# a half-written tree.
WRITTEN=""
tmp_dir=""
finish() {
	local status=$?
	if [[ "${status}" -ne 0 && -n "${WRITTEN}" ]]; then
		git checkout --quiet -- .
		echo "release-bump: the tracked files are restored" >&2
	fi
	if [[ -n "${tmp_dir}" ]]; then
		rm -rf "${tmp_dir}"
	fi
	exit "${status}"
}
trap finish EXIT

CARGO="${CARGO:-cargo}"
STRICT='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
# The newest release of the legacy Python application, published from GitLab without a `v` tag.
LEGACY_FLOOR=0.7.6
CARGO_MANIFEST=Cargo.toml
CARGO_LOCK=Cargo.lock
MESON_BUILD=meson.build
METAINFO=data/com.leinardi.gx52.metainfo.xml.in.in
FILES=("${CARGO_LOCK}" "${CARGO_MANIFEST}" "${METAINFO}" "${MESON_BUILD}")

[[ $# -eq 1 ]] || die "usage: release-bump.sh X.Y.Z"
VERSION="$1"
[[ "${VERSION}" =~ ${STRICT} ]] ||
	die "the version must be X.Y.Z with no leading zeros, no pre-release and no build metadata (got '${VERSION}')"
command -v "${CARGO}" >/dev/null || die "cargo is not on PATH"

ROOT="$(git rev-parse --show-toplevel)"
cd "${ROOT}"
[[ -z "$(git status --porcelain --untracked-files=no)" ]] ||
	die "the tracked files must have no uncommitted changes; the bump starts from a clean tree"

# Whether $1 > $2 as versions (both strict X.Y.Z).
newer() { [[ "$1" != "$2" && "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" ]]; }

# The released versions: the `vX.Y.Z` tags, and the legacy tags without `v`.
mapfile -t V_TAGS < <(git tag --list | sed -n 's/^v//p' | { grep -E "${STRICT}" || true; } | sort -V)
mapfile -t LEGACY_TAGS < <(git tag --list | { grep -E "${STRICT}" || true; } | sort -V)
HIGHEST="$(printf '%s\n' "${LEGACY_FLOOR}" "${V_TAGS[@]}" "${LEGACY_TAGS[@]}" | sort -V | tail -n1)"
newer "${VERSION}" "${HIGHEST}" || die "${VERSION} is not higher than the highest release, ${HIGHEST}"
LAST_TAG=""
if [[ "${#V_TAGS[@]}" -gt 0 ]]; then
	LAST_TAG="v${V_TAGS[-1]}"
fi
released() { printf '%s\n' "${V_TAGS[@]}" | grep -qxF "$1"; }

# The [package] table runs from its header to the next table header; exactly one
# `version = "..."` line in it is rewritten.
tmp_dir="$(mktemp -d)"
read -r count current < <(awk -v version="${VERSION}" '
	/^\[/ { in_table = ($0 == "[package]") }
	in_table && /^version = "[^"]*"$/ { old = $0; $0 = "version = \"" version "\""; n++ }
	{ print > out }
	END { gsub(/^version = "|"$/, "", old); print n + 0, (old == "" ? "-" : old) }
' out="${tmp_dir}/Cargo.toml" "${CARGO_MANIFEST}")
[[ "${count}" == 1 ]] || die "[package] in ${CARGO_MANIFEST} has ${count} version lines, expected 1"
[[ "${current}" =~ ${STRICT} ]] || die "${CARGO_MANIFEST} carries '${current}', not X.Y.Z"
newer "${VERSION}" "${current}" || die "${VERSION} is not higher than the version ${CARGO_MANIFEST} carries, ${current}"

# The project() call opens meson.build and ends at the first `)` alone on a line; its one
# `version: '...'` keyword line is rewritten.
read -r count < <(awk -v version="${VERSION}" '
	/^project\(/ { in_call = 1 }
	in_call && /^    version: '\''[^'\'']*'\'',$/ { $0 = "    version: '\''" version "'\'',"; n++ }
	in_call && /^\)$/ { in_call = 0 }
	{ print > out }
	END { print n + 0 }
' out="${tmp_dir}/meson.build" "${MESON_BUILD}")
[[ "${count}" == 1 ]] || die "the project() call of ${MESON_BUILD} has ${count} version lines, expected 1"

# The text of an <li>, escaped for XML.
xml_escape() { sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g'; }

# The rewrite in a few lines: the notes of the first release, and of the first stable one, which
# is what the users of the legacy 0.7.x see when it replaces it on Flathub stable.
REWRITE=(
	"GX52 is rewritten in Rust with GTK 4 and libadwaita"
	"Per-mode profiles for the LEDs, the MFD and the brightness; GX52 0.7 profiles are imported"
	"Time zones for the MFD clocks, custom MFD text, and profile import and export"
	"A tray icon, running in the background and starting on login"
	"An input tester, a virtual mouse on the throttle ministick, and udev guidance in the app"
)
# The first stable release: no `v` tag of major 1 or higher yet.
first_stable() {
	[[ "${VERSION%%.*}" -ge 1 ]] || return 1
	local version
	for version in "${V_TAGS[@]}"; do
		[[ "${version%%.*}" -eq 0 ]] || return 1
	done
}
if [[ -z "${LAST_TAG}" ]]; then
	ITEMS=("${REWRITE[@]}")
elif first_stable; then
	ITEMS=(
		"${REWRITE[@]}"
		"The profiles of a GX52 0.7 installed from source, not from Flathub, are copied into the Flatpak first: the README shows how"
		"The X52 (non-Pro) is supported on a best-effort basis: it has not been tested on a device"
	)
else
	# Oldest first. A subject's type, scope and `!` go; its first letter is capitalised.
	mapfile -t ITEMS < <(git log --no-merges --reverse --format=%s "${LAST_TAG}..HEAD" |
		sed -nE 's/^(feat|fix)(\([^)]*\))?!?: +(.+)$/\3/p' |
		awk '{ print toupper(substr($0, 1, 1)) substr($0, 2) }')
	if [[ "${#ITEMS[@]}" -eq 0 ]]; then
		ITEMS=("Maintenance release")
	fi
fi
if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
	DATE="$(date -u -d "@${SOURCE_DATE_EPOCH}" +%Y-%m-%d)"
else
	DATE="$(date -u +%Y-%m-%d)"
fi
{
	printf '    <release version="%s" date="%s">\n' "${VERSION}" "${DATE}"
	printf '      <description translate="no">\n'
	printf '        <ul>\n'
	for item in "${ITEMS[@]}"; do
		printf '          <li>%s</li>\n' "$(xml_escape <<<"${item}")"
	done
	printf '        </ul>\n'
	printf '      </description>\n'
	printf '    </release>\n'
} >"${tmp_dir}/entry.xml"

# The <releases> list with the new entry on top and the unpublished entries of the rewrite gone.
# An entry runs from its `<release version="...">` line to its `</release>` line (or is one
# self-closing line).
mapfile -t DROPPED < <(sed -nE 's/^    <release version="([^"]+)".*/\1/p' "${METAINFO}" | while read -r version; do
	if [[ "${version}" == "${VERSION}" ]]; then
		echo "!${version}"
	elif [[ ! "${version}" =~ ^0\.7\. ]] && ! released "${version}"; then
		echo "${version}"
	fi
done)
for version in "${DROPPED[@]}"; do
	[[ "${version}" != "!"* ]] || die "${METAINFO} already has a <release> for ${version#!}"
done
read -r count < <(awk -v entry="${tmp_dir}/entry.xml" -v dropped="${DROPPED[*]}" '
	BEGIN { split(dropped, list, " "); for (i in list) drop[list[i]] = 1 }
	skipping {
		if ($0 ~ /^    <\/release>$/) skipping = 0
		next
	}
	match($0, /^    <release version="[^"]+"/) {
		version = substr($0, 23, RLENGTH - 23)
		if (version in drop) {
			if ($0 !~ /\/>$/) skipping = 1
			next
		}
	}
	{ print > out }
	$0 == "  <releases>" {
		while ((getline line < entry) > 0) print line > out
		n++
	}
	END { print n + 0 }
' out="${tmp_dir}/metainfo" "${METAINFO}")
[[ "${count}" == 1 ]] || die "${METAINFO} has ${count} '  <releases>' lines, expected 1"

WRITTEN=1
cp "${tmp_dir}/Cargo.toml" "${CARGO_MANIFEST}"
cp "${tmp_dir}/meson.build" "${MESON_BUILD}"
cp "${tmp_dir}/metainfo" "${METAINFO}"

# A plain resolve rewrites the root package's entry of Cargo.lock and nothing else; `cargo update`
# could also move dependencies. It needs the registry index, which the workflow's bump job fetches.
"${CARGO}" metadata --format-version 1 >/dev/null || die "cargo could not resolve the workspace"
"${CARGO}" metadata --format-version 1 --locked >/dev/null || die "Cargo.lock is not up to date after the resolve"

changed="$(git diff --name-only | sort)"
[[ "${changed}" == "$(printf '%s\n' "${FILES[@]}" | sort)" ]] ||
	die "expected ${FILES[*]} to change, got: ${changed//$'\n'/, }"
lock_added="$(git diff -U0 "${CARGO_LOCK}" | sed -n '/^+[^+]/p')"
[[ "${lock_added}" == "+version = \"${VERSION}\"" ]] ||
	die "the resolve changed more than the root package's version in ${CARGO_LOCK}: ${lock_added//$'\n'/ }"
# That one line is gx52's: the second, --locked, resolve proved the lock matches the manifests,
# and only gx52's manifest changed.

if [[ "${#DROPPED[@]}" -gt 0 ]]; then
	echo "release-bump: removed the unpublished <release> entries ${DROPPED[*]}"
fi
git diff --stat
echo "release-bump: gx52 is at ${VERSION}"
