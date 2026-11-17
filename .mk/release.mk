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
ifndef MK_LOCAL_RELEASE_INCLUDED
MK_LOCAL_RELEASE_INCLUDED := 1

# Local snippet (NOT part of make-common): the two commands the Release workflow gives
# gh-reusable-workflows' version-bump-pr and release-on-merge (docs/release.md). release-version
# prints the version the tree carries, the root package's in Cargo.toml (`make -s` keeps make's
# own lines out of it); release-bump writes a new one into the four files that carry it
# (scripts/release-bump.sh) and never commits.

.PHONY: release-version
release-version: ## Print the version the tree carries (Cargo.toml [package] version)
	@awk '/^\[/ { in_table = ($$0 == "[package]") } \
		in_table && /^version = "[^"]*"$$/ { gsub(/^version = "|"$$/, ""); print; n++ } \
		END { if (n != 1) { print "Cargo.toml: expected one [package] version, found " n + 0 > "/dev/stderr"; exit 1 } }' \
		Cargo.toml

.PHONY: release-bump
release-bump: ## Write VERSION=X.Y.Z into Cargo.toml, Cargo.lock, meson.build and the metainfo (no commit)
	@test -n "$(VERSION)" || { echo "usage: make release-bump VERSION=X.Y.Z" >&2; exit 1; }
	scripts/release-bump.sh "$(VERSION)"

endif  # MK_LOCAL_RELEASE_INCLUDED
