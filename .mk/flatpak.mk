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

ifndef MK_LOCAL_FLATPAK_INCLUDED
MK_LOCAL_FLATPAK_INCLUDED := 1

# Local snippet (NOT part of make-common): the Flatpak builds, as spike S6 proved them. Every
# builder and linter call goes through the org.flatpak.Builder Flatpak (flatpak-builder and
# flatpak-builder-lint at Flathub's versions), which needs --disable-rofiles-fuse. The Devel
# manifest builds com.leinardi.gx52.Devel from the checkout with the network allowed during the
# build (cargo fetches the crates) and runs meson test; the stable manifest is the offline,
# Flathub-style build, whose crates come from the generated build-aux/cargo-sources.json. With
# --ccache the Devel build keeps cargo's home and target directory in .flatpak-builder/ccache, so
# a rebuild compiles the workspace crates only (src/meson.build).

FLATPAK ?= flatpak
FLATPAK_BUILDER ?= $(FLATPAK) run org.flatpak.Builder
FLATPAK_BUILDER_LINT ?= $(FLATPAK) run --command=flatpak-builder-lint org.flatpak.Builder
UV ?= uv

FLATPAK_DEVEL_ID := com.leinardi.gx52.Devel
FLATPAK_DEVEL_MANIFEST := build-aux/$(FLATPAK_DEVEL_ID).json
FLATPAK_MANIFESTS := build-aux/com.leinardi.gx52.json $(FLATPAK_DEVEL_MANIFEST)
FLATPAK_DIR ?= _flatpak
FLATPAK_REPO := $(FLATPAK_DIR)/repo
FLATPAK_BUNDLE := $(FLATPAK_DIR)/$(FLATPAK_DEVEL_ID).flatpak
FLATPAK_BUILDER_ARGS := --user --force-clean --disable-rofiles-fuse --ccache \
    --install-deps-from=flathub --repo=$(FLATPAK_REPO)

# flatpak-cargo-generator.py from flatpak-builder-tools, pinned by commit and checked by digest
# before it runs. It declares its Python dependencies inline, so uv runs it without a virtualenv.
CARGO_SOURCES := build-aux/cargo-sources.json
CARGO_GENERATOR_COMMIT := 41c20aa10819cdb2a4f3ca171758a96d1955c018
CARGO_GENERATOR_SHA256 := 0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
CARGO_GENERATOR_URL := https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$(CARGO_GENERATOR_COMMIT)/cargo/flatpak-cargo-generator.py
CARGO_GENERATOR := $(FLATPAK_DIR)/tools/flatpak-cargo-generator-$(CARGO_GENERATOR_COMMIT).py

.PHONY: flatpak-build
flatpak-build: ## Build the Devel Flatpak into _flatpak/repo (runs meson test inside the SDK)
	$(FLATPAK_BUILDER) $(FLATPAK_BUILDER_ARGS) $(FLATPAK_DIR)/build $(FLATPAK_DEVEL_MANIFEST)

.PHONY: flatpak-install
flatpak-install: ## Build the Devel Flatpak and install it for the user (flatpak run com.leinardi.gx52.Devel)
	$(FLATPAK_BUILDER) $(FLATPAK_BUILDER_ARGS) --install $(FLATPAK_DIR)/build $(FLATPAK_DEVEL_MANIFEST)

.PHONY: flatpak-bundle
flatpak-bundle: flatpak-build ## Export the Devel build as a single-file bundle, _flatpak/com.leinardi.gx52.Devel.flatpak
	$(FLATPAK) build-bundle $(FLATPAK_REPO) $(FLATPAK_BUNDLE) $(FLATPAK_DEVEL_ID)

# The stable manifest lists cargo-sources.json, so the linter needs it to read that manifest.
.PHONY: flatpak-lint-manifest
flatpak-lint-manifest: $(CARGO_SOURCES) ## Lint the stable and Devel manifests with flatpak-builder-lint
	@set -e; for manifest in $(FLATPAK_MANIFESTS); do \
		echo "flatpak-builder-lint manifest $$manifest"; \
		$(FLATPAK_BUILDER_LINT) manifest "$$manifest"; \
	done

# Not a gate before M8: the repository check also wants the screenshots the metainfo only gets
# with the first Flathub release (docs/roadmap.md, M8), so it is expected to report them missing.
.PHONY: flatpak-lint-repo
flatpak-lint-repo: ## Lint the repository flatpak-build exported (expected to report the missing screenshots until M8)
	@test -d $(FLATPAK_REPO) || { echo "$(FLATPAK_REPO) does not exist; run make flatpak-build first" >&2; exit 1; }
	$(FLATPAK_BUILDER_LINT) repo $(FLATPAK_REPO)

$(CARGO_GENERATOR):
	mkdir -p $(@D)
	curl -fsSL -o $@.download $(CARGO_GENERATOR_URL)
	echo "$(CARGO_GENERATOR_SHA256)  $@.download" | sha256sum --check --quiet
	mv $@.download $@

$(CARGO_SOURCES): Cargo.lock $(CARGO_GENERATOR)
	$(UV) run --script $(CARGO_GENERATOR) Cargo.lock -o $@

.PHONY: cargo-sources
cargo-sources: $(CARGO_SOURCES) ## Regenerate build-aux/cargo-sources.json from Cargo.lock (network required)

endif  # MK_LOCAL_FLATPAK_INCLUDED
