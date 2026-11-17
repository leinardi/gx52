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
# clone lives in FLATHUB_DIR (_flathub, ignored and cloned on demand); each target pushes a branch
# with the maintainer's git login and opens or updates its pull request with gh. VERSION
# defaults to the version the tree carries, which after a release's merge is the released one.

FLATHUB_DIR ?= $(REPO_ROOT)/_flathub
FLATHUB_BASE ?= beta

.PHONY: flathub-pr
flathub-pr: ## Push the Flathub branch of release VERSION into FLATHUB_BASE (beta; master) and open its PR
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh pr \
		"$(or $(VERSION),$(shell $(MAKE) -s --no-print-directory release-version))" "$(FLATHUB_BASE)"

.PHONY: flathub-beta
flathub-beta: ## Create the Flathub beta branch from the passed test build of VERSION (once)
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh beta \
		"$(or $(VERSION),$(shell $(MAKE) -s --no-print-directory release-version))"

.PHONY: flathub-promote
flathub-promote: ## Open the Flathub master PR of release VERSION, 1.0.0 or later (promotion to stable)
	@version="$(or $(VERSION),$(shell $(MAKE) -s --no-print-directory release-version))"; \
	case "$$version" in 0.*) echo "$$version is a 0.x release: it goes to beta only" >&2; exit 1 ;; esac; \
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh pr "$$version" master

.PHONY: flathub-merge
flathub-merge: ## Wait for Flathub's test build of the open FLATHUB_BASE PR, then merge it on confirmation
	FLATHUB_DIR="$(FLATHUB_DIR)" scripts/flathub-release.sh merge "$(FLATHUB_BASE)"

endif  # MK_LOCAL_FLATHUB_INCLUDED
