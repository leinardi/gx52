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
# The version policy of a release (docs/release.md, "Versions and channels"), checked on the raw
# `version` input of the Release workflow before any version is resolved. svu is not used before
# 1.0.0: on a 0.x history it turns a `feat` into 0.100.0 and a breaking change into 1.0.0. So,
# while no `vX.Y.Z` tag is 1.0.0 or higher:
#
#   - an empty input fails ("give the version explicitly before 1.0.0");
#   - an explicit version must be a 0.99.x development build, or 1.0.0 itself.
#
# From the first `v1.0.0` (or higher) tag on, an empty input is allowed (svu derives the version)
# and an explicit one is left to the resolver, which requires it higher than every release.
#
# Usage: scripts/release-policy.sh [VERSION]   (X.Y.Z or vX.Y.Z; empty or absent: derive)
#
# It reads the tags of the repository it runs in, so the checkout needs them (fetch-tags).

set -euo pipefail

die() {
	echo "release-policy: $*" >&2
	exit 1
}

[[ $# -le 1 ]] || die "usage: release-policy.sh [VERSION]"
INPUT="${1:-}"
STRICT='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

VERSION="${INPUT#v}"
if [[ -n "${INPUT}" && ! "${VERSION}" =~ ${STRICT} ]]; then
	die "the version must be X.Y.Z or vX.Y.Z with no leading zeros, no pre-release and no build metadata (got '${INPUT}')"
fi

STABLE="$(git tag --list | sed -n 's/^v//p' | { grep -E "${STRICT}" || true; } |
	awk -F. '$1 >= 1' | sort -V | tail -n1)"

if [[ -n "${STABLE}" ]]; then
	echo "release-policy: v${STABLE} is released, so ${INPUT:-an empty version (svu derives it)} is allowed"
	exit 0
fi
if [[ -z "${INPUT}" ]]; then
	die "give the version explicitly before 1.0.0: svu would turn a feat into 0.100.0 and a breaking change into 1.0.0"
fi
if [[ ! "${VERSION}" =~ ^0\.99\.(0|[1-9][0-9]*)$ && "${VERSION}" != 1.0.0 ]]; then
	die "before 1.0.0 a release is a 0.99.x development build or 1.0.0 itself (got ${VERSION})"
fi
echo "release-policy: ${VERSION} is allowed before 1.0.0"
