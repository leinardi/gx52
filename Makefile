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

# Resolve repository root (Makefile can live anywhere)
REPO_ROOT := $(shell git rev-parse --show-toplevel 2>/dev/null || pwd)

MK_COMMON_REPO        ?= leinardi/make-common
MK_COMMON_VERSION     ?= v1

MK_COMMON_DIR         := $(REPO_ROOT)/.mk

# Shared snippets coming from make-common
MK_COMMON_FILES       := help.mk pre-commit.mk

# Repo-local snippets that are NOT in make-common
MK_LOCAL_FILES        := audit-deps.mk flatpak.mk meson.mk rust.mk verify.mk

MK_COMMON_BOOTSTRAP_SCRIPT := $(REPO_ROOT)/scripts/bootstrap-mk-common.sh

# Bootstrap: the script will self-update and fetch the selected .mk snippets
MK_COMMON_BOOTSTRAP := $(shell "$(MK_COMMON_BOOTSTRAP_SCRIPT)" \
  "$(MK_COMMON_REPO)" \
  "$(MK_COMMON_VERSION)" \
  "$(MK_COMMON_DIR)" \
  "$(MK_COMMON_FILES)")

# -----------------------------------------------------------------------------
# Include shared make logic (fetched from make-common)
# -----------------------------------------------------------------------------
include $(addprefix $(MK_COMMON_DIR)/,$(MK_COMMON_FILES))

# -----------------------------------------------------------------------------
# Include repo-local logic (no bootstrap; lives only in this repo)
# -----------------------------------------------------------------------------
-include $(addprefix $(REPO_ROOT)/.mk/,$(MK_LOCAL_FILES))

.PHONY: mk-common-update
mk-common-update: ## Check for remote updates of shared .mk files
	@echo "[mk] Checking for updates from $(MK_COMMON_REPO)@$(MK_COMMON_VERSION)"
	MK_COMMON_UPDATE=1 "$(MK_COMMON_BOOTSTRAP_SCRIPT)" \
	  "$(MK_COMMON_REPO)" \
	  "$(MK_COMMON_VERSION)" \
	  "$(MK_COMMON_DIR)" \
	  "$(MK_COMMON_FILES)"

# -----------------------------------------------------------------------------
# Adding new targets
# -----------------------------------------------------------------------------
# Do NOT add recipes directly to this file. Instead:
#   - Project-specific targets -> new .mk/<fragment>.mk added to MK_LOCAL_FILES
#   - Generic targets (useful beyond this repo) -> new or updated .mk/<fragment>.mk
#     added to MK_COMMON_FILES, then open a PR to port the change upstream at
#     https://github.com/leinardi/make-common so mk-common-update keeps working.
