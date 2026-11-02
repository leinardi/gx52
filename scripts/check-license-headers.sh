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
# License header checker.
#
# gx52 is GPL-3.0-or-later, and every source file says so in its own header. This fails on any
# tracked source file whose first lines do not carry the GNU GPL version 3 (or later) notice with
# Roberto Leinardi's copyright, so a header copied from another project is caught as surely as a
# missing one.
#
# Governed: Rust and shell sources, the Makefile and the repo-local .mk fragments. Rust means every
# .rs file of every Cargo package: src, tests, build scripts, the meta member. Exempt: the
# make-common files the bootstrap fetches (upstream, under make-common's MIT license: the snippets
# named in the Makefile's MK_COMMON_FILES, and the bootstrap script itself), and Rust files under a
# testdata/ directory, which are fixtures. YAML, JSON, TOML, Markdown, Blueprint, Meson and the data
# files are never governed.
#
# Rust files have two more rules:
#
#   - Code reused from another project of the same author (swarm-device-access, Apache-2.0;
#     swarm-scheduler-exporter, MIT; ...) carries this project's GPL header followed by a
#     provenance comment, "// Adapted from <repo>/<path> at <commit>, originally
#     <MIT|Apache-2.0>-licensed, Copyright (c) <year> Roberto Leinardi", which may wrap over
#     several // lines. The comment is accepted and never required, but one that is present must
#     be complete, so a reused file's origin cannot be recorded half-way.
#   - The MIT permission text and the Apache-2.0 notice must not appear in the header: the reused
#     file is relicensed, and pasting the old block alongside the new one would leave two licences
#     in one file.
#
# Adapted from swarm-device-access/scripts/check-license-headers.sh at 4cd31c4cea79, originally
# Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi.

set -euo pipefail

usage() {
	cat <<'USAGE'
Usage: scripts/check-license-headers.sh

Checks every tracked source file for the gx52 GPL-3.0-or-later header and lists the files that
lack it. Run from anywhere inside the repository.
USAGE
}

if [[ $# -gt 0 ]]; then
	usage >&2
	exit 2
fi

cd "$(git rev-parse --show-toplevel)"

# How many leading lines the header must sit in: a shebang and a blank line may precede it.
HEADER_WINDOW=25

REQUIRED_LINES=(
	"it under the terms of the GNU General Public License as published by"
	"the Free Software Foundation, either version 3 of the License, or"
	"(at your option) any later version."
)
# The year is the file's first commit; a range (2025-2026) is allowed too.
COPYRIGHT_RE='Copyright [0-9]{4}(-[0-9]{4})? Roberto Leinardi\.'

# The provenance comment of a Rust file reused from another project, once its // lines are joined.
PROVENANCE_RE='^Adapted from [A-Za-z0-9._-]+/[^ ]+ at [0-9a-f]{7,40}, originally (MIT|Apache-2\.0)-licensed, Copyright \(c\) [0-9]{4}(-[0-9]{4})? Roberto Leinardi$'
# Text only the licences a reused file was relicensed from carry.
FOREIGN_MARKERS=(
	"Permission is hereby granted, free of charge"
	"Licensed under the Apache License"
)

# Exact paths that are not this repository's own source.
EXEMPT_PATHS=(
	"scripts/bootstrap-mk-common.sh"
)

# The shared make fragments, read from the Makefile so the list cannot drift from what the
# bootstrap actually fetches.
mk_common_files() {
	sed -n 's/^MK_COMMON_FILES[[:space:]]*:=[[:space:]]*//p' Makefile
}

is_governed() {
	local path="$1" base
	base="${path##*/}"

	case "${path}" in
	testdata/*.rs | */testdata/*.rs)
		return 1
		;;
	*.rs | *.sh | .mk/*.mk)
		return 0
		;;
	esac

	case "${base}" in
	Makefile)
		return 0
		;;
	esac

	return 1
}

is_exempt() {
	local path="$1" entry

	for entry in "${EXEMPT_PATHS[@]}"; do
		if [[ ${path} == "${entry}" ]]; then
			return 0
		fi
	done

	for entry in ${COMMON_FILES}; do
		if [[ ${path} == ".mk/${entry}" ]]; then
			return 0
		fi
	done

	return 1
}

has_header() {
	local path="$1" head line
	head="$(head -n "${HEADER_WINDOW}" -- "${path}")"

	for line in "${REQUIRED_LINES[@]}"; do
		if [[ ${head} != *"${line}"* ]]; then
			return 1
		fi
	done

	grep -Eq -- "${COPYRIGHT_RE}" <<<"${head}" || return 1

	if [[ ${path} == *.rs ]]; then
		rust_header_ok "${head}" || return 1
	fi
}

# rust_header_ok applies the two Rust-only rules to a file's header window.
rust_header_ok() {
	local head="$1" provenance marker

	for marker in "${FOREIGN_MARKERS[@]}"; do
		if [[ ${head} == *"${marker}"* ]]; then
			return 1
		fi
	done

	# The comment runs from its "// Adapted from" line to the line that ends in the holder's name,
	# or to the first line that is not a plain // comment.
	provenance="$(awk '
		/^\/\/ Adapted from / { open = 1 }
		open {
			if ($0 !~ /^\/\/( |$)/) { exit }
			text = $0
			sub(/^\/\/ ?/, "", text)
			joined = (joined == "" ? text : joined " " text)
			if (text ~ /Roberto Leinardi$/) { exit }
		}
		END { print joined }
	' <<<"${head}")"

	if [[ -z ${provenance} ]]; then
		return 0
	fi

	grep -Eq -- "${PROVENANCE_RE}" <<<"${provenance}"
}

COMMON_FILES="$(mk_common_files)"
if [[ -z ${COMMON_FILES} ]]; then
	echo "The Makefile names no MK_COMMON_FILES; the make-common exemption would be empty." >&2
	exit 1
fi

missing=()
checked=0

while IFS= read -r -d '' path; do
	if ! is_governed "${path}" || is_exempt "${path}"; then
		continue
	fi

	# A tracked file deleted in the working tree is not this check's concern.
	if [[ ! -f ${path} ]]; then
		continue
	fi

	checked=$((checked + 1))

	if ! has_header "${path}"; then
		missing+=("${path}")
	fi
done < <(git ls-files -z)

if [[ ${#missing[@]} -gt 0 ]]; then
	echo "These files lack the gx52 GPL-3.0-or-later header:" >&2
	printf '  %s\n' "${missing[@]}" >&2
	echo "Copy the header from any Rust or shell file (IntelliJ: the GPL-3.0-or-later copyright profile)." >&2
	echo "A reused Rust file adds a complete provenance comment below it, never the original licence block." >&2
	exit 1
fi

echo "License headers: ${checked} files checked, all carry the GPL-3.0-or-later header."
