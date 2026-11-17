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
ifndef MK_LOCAL_FLATHUB_INCLUDED
MK_LOCAL_FLATHUB_INCLUDED := 1

# Local snippet (NOT part of make-common): the maintainer's pull requests to
# flathub/com.leinardi.gx52 (scripts/flathub-release.sh, docs/release.md "Flathub by hand"). The
# clone lives in FLATHUB_DIR, next to the other Flathub clones; each target pushes a branch with
# the maintainer's git login and prints the link that opens the pull request filled in. VERSION
# defaults to the version the tree carries, which after a release's merge is the released one.

FLATHUB_DIR ?= $(abspath $(REPO_ROOT)/../flathub)
FLATHUB_BASE ?= beta

.PHONY: flathub-pr
flathub-pr: ## Push the Flathub branch of release VERSION into FLATHUB_BASE (beta; master) and print its PR link
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh pr \
		"$(or $(VERSION),$(shell $(MAKE) -s --no-print-directory release-version))" "$(FLATHUB_BASE)"

.PHONY: flathub-beta
flathub-beta: ## Create the Flathub beta branch from the passed test build of VERSION (once)
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh beta \
		"$(or $(VERSION),$(shell $(MAKE) -s --no-print-directory release-version))"

.PHONY: flathub-workflow
flathub-workflow: ## Push the branch adding update.yml to the Flathub master and print its PR link
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh workflow

endif  # MK_LOCAL_FLATHUB_INCLUDED
