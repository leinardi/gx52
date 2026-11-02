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

ifndef MK_LOCAL_MESON_INCLUDED
MK_LOCAL_MESON_INCLUDED := 1

# Local snippet (NOT part of make-common): the application as Meson builds it on the host, in the
# Devel profile (com.leinardi.gx52.Devel). Meson drives cargo for the gx52 package, compiles the
# Blueprint templates, the GResource bundle and the GSettings schema, and its tests validate the
# desktop file, the metainfo and the schema next to cargo test. The host needs the GTK 4 and
# libadwaita development files, blueprint-compiler, desktop-file-utils and appstream (AGENTS.md).

MESON ?= meson
MESON_BUILD_DIR ?= _build

# The configured build directory. Meson reconfigures itself when a meson.build changes, so setup
# runs once per directory.
$(MESON_BUILD_DIR)/build.ninja:
	$(MESON) setup $(MESON_BUILD_DIR) -Dprofile=development

.PHONY: meson-setup
meson-setup: $(MESON_BUILD_DIR)/build.ninja ## Configure the Devel build in _build (MESON_BUILD_DIR)

.PHONY: meson-compile
meson-compile: meson-setup ## Build the Devel application with Meson
	$(MESON) compile -C $(MESON_BUILD_DIR)

.PHONY: meson-test
meson-test: meson-setup ## Run meson test (cargo test of gx52 and x52, desktop, metainfo and schema validation)
	$(MESON) test -C $(MESON_BUILD_DIR) --print-errorlogs

.PHONY: meson-run
meson-run: meson-compile ## Run the Devel build uninstalled with meson devenv (build-dir resources and schema)
	$(MESON) devenv -C $(MESON_BUILD_DIR) ./src/gx52

.PHONY: meson-install
meson-install: meson-compile ## Install the Devel build into its prefix (DESTDIR honoured)
	$(MESON) install -C $(MESON_BUILD_DIR)

endif  # MK_LOCAL_MESON_INCLUDED
