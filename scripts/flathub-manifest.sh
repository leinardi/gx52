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
# Turns the stable manifest of a tagged tree (build-aux/com.leinardi.gx52.json) into the manifest
# of the Flathub repository (docs/release.md, "The Flathub pull request"). The Flathub branch's
# manifest is replaced by it, not patched, so the legacy Python manifest leaves nothing behind.
#
# Only the app's own source changes: the one `dir` source of the `gx52` module becomes
#
#   {"type": "git", "url": "https://github.com/leinardi/gx52.git", "tag": TAG, "commit": COMMIT}
#
# in the same place. Everything else (the cargo-sources.json entry, the inline .cargo/config.toml
# that vendors the crates, the runtime, the SDK extensions and the finish-args) is the tagged
# manifest's, unchanged. It fails unless the manifest has exactly one `gx52` module with exactly
# one `dir` source, and leaves no `dir` source anywhere.
#
# A dry run builds the result before a tag exists: with --source-dir PATH the `dir` source stays
# as it is (its `skip` list included) but points at PATH (the checkout of the tree being released,
# relative to where the manifest is written), and everything else is the same.
#
# Usage: scripts/flathub-manifest.sh MANIFEST TAG COMMIT
#        scripts/flathub-manifest.sh MANIFEST --source-dir PATH
# Writes the manifest to stdout, indented with four spaces as Flathub's manifests are.

set -euo pipefail

die() {
	echo "flathub-manifest: $*" >&2
	exit 1
}

REPOSITORY_URL=https://github.com/leinardi/gx52.git

[[ $# -eq 3 ]] || die "usage: flathub-manifest.sh MANIFEST TAG COMMIT | MANIFEST --source-dir PATH"
MANIFEST="$1"
[[ -f "${MANIFEST}" ]] || die "${MANIFEST} is not a file"
command -v jq >/dev/null || die "jq is not on PATH"

if [[ "$2" == --source-dir ]]; then
	[[ -n "$3" ]] || die "--source-dir needs a path"
	SOURCE="$(jq -n --arg path "$3" '{type: "dir", path: $path}')"
else
	TAG="$2"
	COMMIT="$3"
	[[ "${TAG}" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
		die "the tag must be vX.Y.Z (got '${TAG}')"
	[[ "${COMMIT}" =~ ^[0-9a-f]{40}$ ]] || die "the commit must be a full 40-hex SHA (got '${COMMIT}')"
	SOURCE="$(jq -n --arg url "${REPOSITORY_URL}" --arg tag "${TAG}" --arg commit "${COMMIT}" \
		'{type: "git", url: $url, tag: $tag, commit: $commit}')"
fi

jq --indent 4 --argjson source "${SOURCE}" '
	def is_dir: type == "object" and .type == "dir";
	if (.modules | type) != "array" then error("the manifest has no modules list") else . end
	| ([.modules[] | select(type == "object" and .name == "gx52")] | length) as $apps
	| if $apps != 1 then error("the manifest has \($apps) gx52 modules, expected 1") else . end
	| .modules |= map(
		if type == "object" and .name == "gx52" then
			([.sources[]? | select(is_dir)] | length) as $dirs
			| if $dirs != 1 then error("the gx52 module has \($dirs) dir sources, expected 1") else . end
			| .sources |= map(if is_dir then (if $source.type == "dir" then . + {path: $source.path} else $source end) else . end)
		else . end)
	| if $source.type != "dir" and ([.. | select(is_dir)] | length) > 0
		then error("a dir source is left outside the gx52 module") else . end
' "${MANIFEST}" || die "could not migrate ${MANIFEST}"
