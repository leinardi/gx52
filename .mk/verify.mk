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

ifndef MK_LOCAL_VERIFY_INCLUDED
MK_LOCAL_VERIFY_INCLUDED := 1

# Local snippet (NOT part of make-common): one command that runs every check a change has to pass,
# the way CI does (AGENTS.md, Validation matrix). verify-rust lives in rust.mk, meson-test in
# meson.mk.

.PHONY: verify
verify: verify-rust meson-test ## Verify everything (build, fmt, clippy, tests, docs, meson test)

endif  # MK_LOCAL_VERIFY_INCLUDED
