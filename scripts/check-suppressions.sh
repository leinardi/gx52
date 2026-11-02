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
# Suppression ratchet.
#
# Every linter in this repository can be silenced in place, and each silencing is a small decision
# nobody revisits. This script makes the total a reviewed number instead: it counts suppressions
# per rule and compares them against a committed baseline.
#
# Two rules, and the second is the one that matters:
#
#   1. No count may exceed its baseline, and no rule may appear that the baseline does not know.
#      Counts are per rule: one per Rust lint, and one per code of a shellcheck directive.
#      Deleting a suppression of one rule therefore cannot pay for adding a
#      suppression of another.
#   2. A staged diff that adds a suppression line is refused unless the same commit also changes
#      the baseline file. Rule 1 alone is blind to a swap within the same rule (delete one
#      #[expect(clippy::too_many_lines)], add another), and the baseline file is the thing a
#      reviewer reads.
#
# A count below its baseline is also refused: the baseline is meant to describe the repository, and
# a stale one quietly buys room for a suppression nobody approved. Run with --update to rewrite it,
# and commit that with the change that earned it. --update also refreshes the "reviewed" stamp at
# the top of the file, which is what a commit that only rearranges suppressions -- a swap, a moved
# file -- has to stage: those leave every count where it was, so without the stamp rule 2 would
# refuse with nothing to change.
#
# These three families -- Rust's #[expect] and #[allow], and the shellcheck disable directive -- are
# the mechanisms this repository's linters read in Rust and shell. The two Rust families key on the lint path as written, so
# expect/clippy::unwrap_used and expect/unwrap_used would be different rules; a cfg_attr-wrapped
# lint attribute is not counted. Others exist -- markdownlint-disable (the pull request template
# holds one), yamllint's disable-line, the shellcheck directives inside workflow run: blocks -- and
# are not counted; adding a family is a change to this script and its tests.
#
# Rust files under a testdata/ directory are fixtures, data rather than code, and are not scanned
# (is_scanned). The exclusion covers Rust files only: shell scripts under testdata/ are counted like
# any other.
#
# Adapted from swarm-device-access/scripts/check-suppressions.sh at 4cd31c4cea79, originally
# Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi; the droast families are dropped, since
# this repository has no Dockerfile.
#
# scripts/check-suppressions.sh is to be exercised by meta/tests/suppressions.rs, which drives it
# against scratch repositories rather than this one (docs/tooling-todo.md).

set -euo pipefail

BASELINE_FILE="${SUPPRESSIONS_BASELINE:-.suppressions-baseline}"

# detect_program reads source lines and prints one family key per suppression found, as
# "<path>:<line><TAB><key>". Line and path are what makes a failure actionable; the counting mode
# throws them away again. It reads either a list of files, taking each path from FILENAME, or one
# file on standard input whose name is given as -v override.
#
# The program is read from a quoted heredoc, so its $0 and its regexes reach awk exactly as
# written without a single-quoted string the shell would have to be told not to expand.
read -r -d '' detect_program <<'AWK' || true
BEGIN {
	quote_char = sprintf("%c", 39)
}

function emit(key) {
	printf "%s:%d\t%s\n", path, FNR, key
}

# --- Rust ---
#
# A Rust lint attribute counts wherever it sits in code: outer or inner, on a line of its own or
# after other tokens, and spread over several lines the way rustfmt wraps a long one. Each lint it
# names is a rule of its own, reported on the line that names it, so adding a lint to an existing
# multi-line attribute is an addition too. The reason = "..." item is not a lint and is skipped.
#
# Strings, raw strings, character literals and (nesting) block comments are tracked across lines,
# so attribute text inside a fixture or a comment is not a suppression.

function rs_finish_item(   name) {
	name = attr_item
	gsub(/[ \t]/, "", name)
	if (name ~ /^[A-Za-z_][A-Za-z0-9_]*(::[A-Za-z_][A-Za-z0-9_]*)*$/) {
		printf "%s:%d\t%s/%s\n", path, attr_item_line, attr_family, name
	}
	attr_item = ""
	attr_item_line = 0
}

# rs_attr_char feeds one character of an attribute's argument list, outside any string or comment.
function rs_attr_char(c) {
	if (c == "(") {
		attr_item = attr_item c
		attr_depth++
		return
	}
	if (c == ")") {
		attr_depth--
		if (attr_depth == 0) {
			rs_finish_item()
			attr_family = ""
			return
		}
		attr_item = attr_item c
		return
	}
	if (c == "," && attr_depth == 1) {
		rs_finish_item()
		return
	}
	if (attr_item_line == 0 && c !~ /[ \t]/) {
		attr_item_line = FNR
	}
	attr_item = attr_item c
}

function rs_scan(s,   i, n, c, two, closing, token, prev) {
	n = length(s)
	i = 1

	while (i <= n) {
		c = substr(s, i, 1)
		two = substr(s, i, 2)

		if (rs_block > 0) {
			if (two == "/*") {
				rs_block++
				i += 2
			} else if (two == "*/") {
				rs_block--
				i += 2
			} else {
				i++
			}
			continue
		}

		if (rs_raw >= 0) {
			closing = "\""
			for (token = 0; token < rs_raw; token++) {
				closing = closing "#"
			}
			if (substr(s, i, length(closing)) == closing) {
				rs_raw = -1
				i += length(closing)
			} else {
				i++
			}
			continue
		}

		if (rs_str) {
			if (c == "\\") {
				i += 2
			} else {
				if (c == "\"") {
					rs_str = 0
				}
				i++
			}
			continue
		}

		if (two == "//") {
			return
		}

		if (two == "/*") {
			rs_block = 1
			i += 2
			continue
		}

		prev = (i > 1 ? substr(s, i - 1, 1) : "")
		if ((c == "r" || c == "b") && prev !~ /[A-Za-z0-9_]/ && match(substr(s, i), /^b?r#*"/)) {
			token = substr(s, i, RLENGTH)
			rs_raw = gsub(/#/, "", token)
			if (attr_family != "") {
				attr_item = attr_item "\""
			}
			i += RLENGTH
			continue
		}

		if (c == "\"") {
			rs_str = 1
			if (attr_family != "") {
				attr_item = attr_item c
			}
			i++
			continue
		}

		if (c == quote_char) {
			if (substr(s, i + 1, 1) == "\\") {
				token = index(substr(s, i + 3), quote_char)
				i = (token > 0 ? i + 3 + token : n + 1)
			} else if (substr(s, i + 2, 1) == quote_char) {
				i += 3
			} else {
				# A lifetime or a label.
				i++
			}
			continue
		}

		if (attr_family != "") {
			rs_attr_char(c)
			i++
			continue
		}

		if (c == "#" && match(substr(s, i), /^#!?\[[ \t]*(expect|allow)[ \t]*\(/)) {
			token = substr(s, i, RLENGTH)
			attr_family = (token ~ /expect/ ? "expect" : "allow")
			attr_depth = 1
			attr_item = ""
			attr_item_line = 0
			i += RLENGTH
			continue
		}

		i++
	}
}

FNR == 1 {
	rs_block = 0
	rs_raw = -1
	rs_str = 0
	attr_family = ""
}

{
	line = $0
	path = (override != "" ? override : FILENAME)

	if (path ~ /\.rs$/) {
		rs_scan(line)
		next
	}

	if (path ~ /\.sh$/) {
		if (match(line, /#[ \t]*shellcheck[ \t]+disable=[A-Za-z0-9,]+/)) {
			directive = substr(line, RSTART, RLENGTH)
			sub(/^.*=/, "", directive)
			count = split(directive, codes, ",")
			for (i = 1; i <= count; i++) {
				if (codes[i] != "") {
					emit("shellcheck-disable/" codes[i])
				}
			}
		}
		next
	}
}
AWK

usage() {
	cat <<'EOF'
Usage: scripts/check-suppressions.sh [--update]

  (no flags)  Check the working tree and the staged diff against the baseline.
  --update    Rewrite the baseline from the current tree, for review and commit.
EOF
}

# is_scanned reports whether a path is one of the file kinds the detector understands. A Rust file
# under a testdata/ directory is a fixture and is not scanned; no other kind has that exclusion.
is_scanned() {
	case "$1" in
	testdata/*.rs | */testdata/*.rs) return 1 ;;
	*.rs | *.sh) return 0 ;;
	*) return 1 ;;
	esac
}

# scanned_files fills the SCANNED array with the tracked sources the ratchet counts.
#
# Paths come from git NUL-separated: core.quotePath would otherwise hand back a C-quoted name for
# anything non-ASCII, and the ratchet would fail on a path rather than on a suppression.
scanned_files() {
	local path

	SCANNED=()

	while IFS= read -r -d '' path; do
		is_scanned "$path" || continue
		SCANNED+=("$path")
	done < <(git ls-files -z -- '*.rs' '*.sh')
}

# current_counts prints the tree's suppression counts as "<key><TAB><count>", sorted by key.
current_counts() {
	scanned_files

	if ((${#SCANNED[@]} == 0)); then
		return 0
	fi

	awk "$detect_program" "${SCANNED[@]}" |
		cut -f2 |
		sort |
		uniq -c |
		awk '{ printf "%s\t%s\n", $2, $1 }'
}

# baseline_counts prints the committed baseline without its comments or blank lines.
baseline_counts() {
	if [[ ! -f $BASELINE_FILE ]]; then
		printf 'suppression baseline %s is missing; run scripts/check-suppressions.sh --update\n' \
			"$BASELINE_FILE" >&2
		exit 1
	fi

	grep -v -e '^#' -e '^[[:space:]]*$' "$BASELINE_FILE" | sort || true
}

# next_review_serial prints the number the next baseline carries: one more than the one it holds
# now, or 1 for a baseline that has none yet.
next_review_serial() {
	local current=0

	if [[ -f $BASELINE_FILE ]]; then
		current="$(awk '/^# reviewed [0-9]+/ { print $3 + 0; exit }' "$BASELINE_FILE")"
	fi

	printf '%d\n' "$((current + 1))"
}

# write_baseline rewrites the baseline file from the current tree.
write_baseline() {
	local counts serial

	# Both are read before the redirection below truncates the file the serial comes from.
	counts="$(current_counts)"
	serial="$(next_review_serial)"

	{
		cat <<'EOF'
# Suppression baseline -- one line per rule, "<family>/<rule><TAB><count>".
#
# Every number here is a standing decision to let a linter stay silent somewhere. It may go down
# freely and may only go up with a reviewer's agreement, which is the whole point of the file being
# committed: scripts/check-suppressions.sh refuses a commit that adds a suppression line without
# changing this file, so raising a count is a visible act rather than a side effect.
#
# Regenerate with: scripts/check-suppressions.sh --update
EOF
		# The stamp is what a commit that only rearranges suppressions has to change. Swapping one
		# suppression for another, or moving a file that carries one, leaves every count untouched,
		# so without a line that moves there would be nothing to stage and the refusal would have
		# no way out but skipping the hook. It counts reviews rather than reading the clock, so
		# that two runs in the same second still differ.
		printf '# reviewed %s, %s\n' "$serial" "$(date -u +%Y-%m-%d)"
		# An empty tree writes no rows, and no blank line in their place.
		if [[ -n $counts ]]; then
			printf '%s\n' "$counts"
		fi
	} >"$BASELINE_FILE"

	printf 'wrote %s\n' "$BASELINE_FILE"
}

# compare_with_baseline fails when any rule exceeds its baseline, is unknown to it, or has fallen
# below it without the file being updated.
compare_with_baseline() {
	local failed=0
	local current baseline

	# Checked explicitly: main runs this function on the left of ||, where set -e is off, so a
	# failure inside a command substitution would be thrown away -- and an empty tree compared
	# with a missing baseline would then pass.
	current="$(current_counts)" || return 1
	baseline="$(baseline_counts)" || return 1

	local key count expected
	while IFS=$'\t' read -r key count; do
		[[ -n $key ]] || continue

		expected="$(printf '%s\n' "$baseline" | awk -F'\t' -v key="$key" '$1 == key { print $2 }')"

		if [[ -z $expected ]]; then
			printf '%s: %d suppression(s), and the baseline does not list this rule at all\n' \
				"$key" "$count" >&2
			failed=1
			continue
		fi

		if ((count > expected)); then
			printf '%s: %d suppression(s), baseline allows %d\n' "$key" "$count" "$expected" >&2
			failed=1
			continue
		fi

		if ((count < expected)); then
			printf '%s: %d suppression(s), baseline still claims %d -- run --update\n' \
				"$key" "$count" "$expected" >&2
			failed=1
		fi
	done <<<"$current"

	while IFS=$'\t' read -r key count; do
		[[ -n $key ]] || continue

		if ! printf '%s\n' "$current" | awk -F'\t' -v key="$key" '$1 == key { found = 1 } END { exit !found }'; then
			printf '%s: gone from the tree, baseline still claims %s -- run --update\n' \
				"$key" "$count" >&2
			failed=1
		fi
	done <<<"$baseline"

	if ((failed != 0)); then
		printf '\nThe suppression counts and %s disagree. Remove the suppression, or run\n' \
			"$BASELINE_FILE" >&2
		printf 'scripts/check-suppressions.sh --update and commit the baseline with the change.\n' >&2
		return 1
	fi

	return 0
}

# added_line_ranges prints "<first> <last>" for every run of lines one staged file gained.
#
# -U0 reduces the diff to the added lines, and the hunk headers say where they land in the staged
# file. A hunk that adds nothing (a pure deletion, whose + count is 0) contributes no range.
#
# A renamed file is passed both of its names: git detects a rename only when it can see both sides,
# and a pathspec naming the new one alone would present a moved file as wholly new, reporting every
# suppression it already carried as freshly added.
added_line_ranges() {
	local path="$1" previous="${2-}"
	local paths=("$path")

	if [[ -n $previous ]]; then
		paths+=("$previous")
	fi

	git diff --cached -U0 -M -- "${paths[@]}" | awk '
		/^@@/ {
			match($0, /\+[0-9]+(,[0-9]+)?/)
			spec = substr($0, RSTART + 1, RLENGTH - 1)
			parts = split(spec, bounds, ",")
			start = bounds[1] + 0
			length_ = (parts > 1 ? bounds[2] + 0 : 1)
			if (length_ > 0) {
				printf "%d %d\n", start, start + length_ - 1
			}
		}
	'
}

# suppressions_added_to prints the suppressions one staged file gained, as "<path>:<line><TAB><key>".
#
# The detector reads the whole staged file rather than the diff's added lines, because it carries
# state across lines: a raw string or a block comment opened earlier decides whether a line is code
# at all, and a stream of fragments would answer that question wrongly in both directions -- a line
# added inside a raw-string fixture would be reported as a new suppression nobody could act on, and
# a lone added raw-string terminator would blind the rest of the file. The diff then only says which
# of the file's lines are new.
suppressions_added_to() {
	local path="$1" previous="${2-}" ranges
	ranges="$(added_line_ranges "$path" "$previous")"

	if [[ -z $ranges ]]; then
		return 0
	fi

	git show ":$path" | awk -v override="$path" "$detect_program" | awk -F'\t' -v ranges="$ranges" '
		BEGIN {
			count = split(ranges, rows, "\n")
			for (i = 1; i <= count; i++) {
				split(rows[i], bounds, " ")
				low[i] = bounds[1] + 0
				high[i] = bounds[2] + 0
			}
		}
		{
			fields = split($1, located, ":")
			line = located[fields] + 0
			for (i = 1; i <= count; i++) {
				if (line >= low[i] && line <= high[i]) {
					print
					next
				}
			}
		}
	'
}

# check_staged_additions refuses a staged diff that adds a suppression line while leaving the
# baseline untouched. This is what catches a swap that leaves the counts unchanged.
check_staged_additions() {
	local staged=() previous_names=()
	local status path previous

	# --name-status -M reports a rename as one entry carrying both names, which added_line_ranges
	# needs to see the move as a move.
	while IFS= read -r -d '' status; do
		IFS= read -r -d '' path || break
		previous=""

		if [[ $status == R* || $status == C* ]]; then
			previous="$path"
			IFS= read -r -d '' path || break
		fi

		staged+=("$path")
		previous_names+=("$previous")
	done < <(git diff --cached -z --name-status -M --diff-filter=ACMR)

	if ((${#staged[@]} == 0)); then
		return 0
	fi

	for path in "${staged[@]}"; do
		if [[ $path == "$BASELINE_FILE" ]]; then
			return 0
		fi
	done

	local added=0 diff_added index
	for index in "${!staged[@]}"; do
		path="${staged[index]}"

		is_scanned "$path" || continue

		diff_added="$(suppressions_added_to "$path" "${previous_names[index]}")"

		if [[ -n $diff_added ]]; then
			printf '%s\n' "$diff_added" >&2
			added=1
		fi
	done

	if ((added != 0)); then
		printf '\nThis commit adds the suppression(s) above without touching %s.\n' \
			"$BASELINE_FILE" >&2
		printf 'Run scripts/check-suppressions.sh --update and commit the baseline with them, so\n' >&2
		printf 'the decision is reviewed rather than absorbed by a count that happens to balance.\n' >&2
		printf 'It refreshes the reviewed stamp even when no count moved, so a swap or a moved file\n' >&2
		printf 'leaves something to stage.\n' >&2
		return 1
	fi

	return 0
}

main() {
	case "${1-}" in
	--update)
		write_baseline
		return 0
		;;
	-h | --help)
		usage
		return 0
		;;
	"") ;;
	*)
		usage >&2
		return 2
		;;
	esac

	local failed=0

	compare_with_baseline || failed=1
	check_staged_additions || failed=1

	return "$failed"
}

main "$@"
