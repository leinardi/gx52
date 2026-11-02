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
# Adapted from swarm-device-access/scripts/hooks/rustfmt-on-edit.sh at 4cd31c4cea79, originally
# Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi.
#

# rustfmt-on-edit.sh -- Claude Code PostToolUse hook (configured in .claude/settings.json).
# Auto-formats Rust and Blueprint files immediately after an Edit/Write tool call so formatting
# drift is fixed at edit time instead of failing later at pre-commit (cargo-fmt and
# blueprint-format hooks).
#
# Input: hook payload JSON on stdin; the edited path is at .tool_input.file_path.
# rustfmt finds the repository's rustfmt.toml (edition, max_width) from the file's directory.
# Always exits 0 (a formatting fix must never block the agent).

set -euo pipefail

file="$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null || true)"

case "$file" in
  *.rs)
    if [[ -f "$file" ]] && command -v rustfmt >/dev/null 2>&1; then
      rustfmt --quiet "$file" || true
    fi
    ;;
  *.blp)
    if [[ -f "$file" ]] && command -v blueprint-compiler >/dev/null 2>&1; then
      blueprint-compiler format --fix "$file" >/dev/null 2>&1 || true
    fi
    ;;
esac

exit 0
