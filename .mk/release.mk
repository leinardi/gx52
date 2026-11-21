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
# gh-reusable-workflows' version-bump-pr and release-on-merge (docs/internal/release.md). release-version
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

# release drives a whole release with the maintainer's gh login (scripts/release.sh): it resumes
# where a previous run stopped, so after a failure it is run again, unchanged, once the cause is
# fixed. FLATHUB_BASE (.mk/flathub.mk) picks the Flathub branches: beta (the default), master
# (1.0.0 or later), or "beta master". With no VERSION, the version is the one the Release workflow
# derives with svu from the Conventional Commits since the last release (1.0.0 on), read back from
# the bump pull request it opens; a run that stopped before the release was published resumes
# without VERSION too. release-dry-run dispatches the dry run of the current branch.

.PHONY: release
release: ## Release [VERSION=X.Y.Z] (none = derived by svu) end to end, with its Flathub PR into FLATHUB_BASE (resumable)
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/release.sh "$(VERSION)" "$(FLATHUB_BASE)"

.PHONY: release-dry-run
release-dry-run: ## Dispatch the release dry run of [VERSION=X.Y.Z] (none = derived by svu) on the current branch
	scripts/release.sh "$(VERSION)" --dry-run

endif  # MK_LOCAL_RELEASE_INCLUDED
