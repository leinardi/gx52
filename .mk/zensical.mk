#
# MIT License
#
# Copyright (c) 2026 Roberto Leinardi
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.
#

ifndef MK_COMMON_ZENSICAL_INCLUDED
MK_COMMON_ZENSICAL_INCLUDED := 1

# Docs site built with Zensical (https://zensical.org). The Zensical counterpart of mkdocs.mk,
# with the same docs-* targets: include one or the other, not both.
#
# Both lock files are usually gitignored, so each has a real file rule: a fresh clone compiles
# the locks before anything syncs from them, and an existing lock is only recompiled (without
# --upgrade) when what it is compiled from is newer.
#
# The Zensical lock is resolved from the tools .in and the Zensical .in together, and the venv
# is synced to that one lock. Two separately compiled locks drift apart on the packages they
# share (zensical needs pygments>=2.20 while an older tools lock pins 2.19.2 through rich),
# and pip-sync refuses two pins for the same package. The tools lock is still compiled and
# installed first, because pip-tools has to be in the venv before the combined lock can be.
#
# uv mode: set DOCS_UV_PROJECT to a directory holding a pyproject.toml (no build system) that
# lists zensical, and its uv.lock, both committed (tools/docs is the suggested place). Then every
# docs-* target runs through uv instead: nothing goes into the system Python (uv keeps its own
# interpreters and caches), uv makes the environment in DOCS_UV_PROJECT/.venv (gitignore it)
# from the lock, hashes included, and
# Dependabot's uv ecosystem can bump zensical in the lock. The pip-tools variables, the tools lock
# and venv/ are not used then.

# Paths / files (override these in your repo if needed)
ZENSICAL_CONFIG ?= zensical.toml

ZENSICAL_IN   ?= zensical-requirements.in
ZENSICAL_LOCK ?= zensical-requirements.txt

DOCS_TOOLS_IN   ?= requirements/tools.in
DOCS_TOOLS_LOCK ?= requirements/tools.txt

# Must match site_dir in the Zensical config (Zensical's default is site)
DOCS_SITE_DIR ?= site

DOCS_PYTHON      ?= $(shell command -v python3 >/dev/null 2>&1 && echo python3 || echo python)
DOCS_PYTHON_VENV ?= venv

DOCS_VENV_BIN := $(DOCS_PYTHON_VENV)/bin
DOCS_VENV_PIP := $(DOCS_VENV_BIN)/pip
DOCS_VENV_PY  := $(DOCS_VENV_BIN)/python

DOCS_PIP_COMPILE       := $(DOCS_VENV_BIN)/pip-compile
DOCS_PIP_COMPILE_FLAGS := --generate-hashes --strip-extras

# uv mode (see above); empty keeps the pip-tools venv.
DOCS_UV_PROJECT ?=
DOCS_UV_BIN     ?= uv
DOCS_UV_RUN      = $(DOCS_UV_BIN) run --project "$(DOCS_UV_PROJECT)" --locked
# pip-audit for docs-audit in uv mode, run with uv tool run at this version.
DOCS_PIP_AUDIT_VERSION ?= 2.10.1

# docs-serve address (IP:PORT); empty uses Zensical's default, localhost:8000.
DOCS_ADDR ?=

# Set DOCS_UV_PROJECT before including this file (or on the command line): the mode is fixed
# when make reads it.
DOCS_UV_MODE := $(strip $(DOCS_UV_PROJECT))

# ---- Python / venv bootstrap -------------------------------------------------

.PHONY: docs-check-uv
docs-check-uv: # Internal: ensure uv exists (uv mode)
	@if ! command -v $(DOCS_UV_BIN) >/dev/null 2>&1; then \
	  echo "Error: uv not found (DOCS_UV_PROJECT is set). Install it: https://docs.astral.sh/uv/"; \
	  exit 1; \
	fi

.PHONY: docs-check-python
docs-check-python: # Internal: ensure Python exists (and that uv mode was not asked for too late)
	$(if $(strip $(DOCS_UV_PROJECT)),$(error DOCS_UV_PROJECT was set after zensical.mk was included; set it before the include or on the command line))
	@if ! command -v $(DOCS_PYTHON) >/dev/null 2>&1; then \
	  echo "Error: Python 3 not found."; \
	  echo "Install it with:"; \
	  echo "  macOS:  brew install python"; \
	  echo "  Ubuntu: sudo apt-get update && sudo apt-get install -y python3 python3-venv python3-pip"; \
	  exit 1; \
	fi

# Stamp file to avoid /bin/python confusion + PEP 668 issues
$(DOCS_PYTHON_VENV)/.ready: | docs-check-python
	@echo "Creating venv at $(DOCS_PYTHON_VENV) (if missing)..."
	$(DOCS_PYTHON) -m venv $(DOCS_PYTHON_VENV)
	$(DOCS_VENV_PY) -m pip install --upgrade pip
	@touch $@

.PHONY: docs-venv
docs-venv: $(if $(DOCS_UV_MODE),docs-deps-sync,$(DOCS_PYTHON_VENV)/.ready) ## Ensure Python virtualenv for docs exists

# pip-tools is needed to compile the tools lock, which pins pip-tools itself, so the first
# install is unpinned; docs-dev-sync then replaces it with the pinned version.
$(DOCS_PIP_COMPILE): | docs-venv
	$(DOCS_VENV_PIP) install pip-tools

# ---- Dev tools (pip-tools, pip-audit, etc.) ----------------------------------

$(DOCS_TOOLS_LOCK): $(DOCS_TOOLS_IN) | docs-venv $(DOCS_PIP_COMPILE)
	@echo "Compiling $(DOCS_TOOLS_IN) -> $(DOCS_TOOLS_LOCK) ..."
	$(DOCS_PIP_COMPILE) $(DOCS_PIP_COMPILE_FLAGS) -o $(DOCS_TOOLS_LOCK) $(DOCS_TOOLS_IN)

.PHONY: docs-dev-lock
docs-dev-lock: | $(if $(DOCS_UV_MODE),,docs-venv $(DOCS_PIP_COMPILE)) ## Re-lock dev tools requirements (tools.in -> tools.txt)
ifneq ($(DOCS_UV_MODE),)
	@echo "uv mode: no tools lock (docs-audit runs pip-audit with uv tool run)."
else
	@echo "Compiling $(DOCS_TOOLS_IN) -> $(DOCS_TOOLS_LOCK) ..."
	$(DOCS_PIP_COMPILE) $(DOCS_PIP_COMPILE_FLAGS) -o $(DOCS_TOOLS_LOCK) $(DOCS_TOOLS_IN)
endif

# The tools lock is installed only when it changes, tracked by a stamp in the venv. The
# combined Zensical lock that docs-deps-sync applies afterwards can pin shared packages
# differently, so installing the tools lock on every build would downgrade them (with a pip
# resolver error) only for pip-sync to upgrade them straight back.
DOCS_TOOLS_STAMP := $(DOCS_PYTHON_VENV)/.tools-synced

$(DOCS_TOOLS_STAMP): $(DOCS_TOOLS_LOCK) | docs-venv
	@echo "Installing dev tools from $(DOCS_TOOLS_LOCK) ..."
	$(DOCS_VENV_PIP) install --upgrade pip
	$(DOCS_VENV_PIP) install -r $(DOCS_TOOLS_LOCK)
	@touch $@

.PHONY: docs-dev-sync
docs-dev-sync: $(if $(DOCS_UV_MODE),docs-check-uv,$(DOCS_TOOLS_STAMP)) ## Install dev tools from tools.txt into the venv (when it changed)

.PHONY: docs-bootstrap
docs-bootstrap: docs-dev-sync ## One-shot docs tooling bootstrap
ifneq ($(DOCS_UV_MODE),)
	@echo "Docs bootstrap complete (uv mode)."
else
	@echo "Docs bootstrap complete."
endif

# ---- Zensical dependencies ---------------------------------------------------

$(ZENSICAL_LOCK): $(DOCS_TOOLS_IN) $(ZENSICAL_IN) | docs-bootstrap
	@echo "Compiling $(DOCS_TOOLS_IN) + $(ZENSICAL_IN) -> $(ZENSICAL_LOCK) ..."
	$(DOCS_PIP_COMPILE) $(DOCS_PIP_COMPILE_FLAGS) -o $(ZENSICAL_LOCK) $(DOCS_TOOLS_IN) $(ZENSICAL_IN)

.PHONY: docs-deps-lock
docs-deps-lock: docs-bootstrap ## Re-lock Zensical deps (no version upgrades)
ifneq ($(DOCS_UV_MODE),)
	$(DOCS_UV_BIN) lock --project "$(DOCS_UV_PROJECT)"
else
	@echo "Compiling $(DOCS_TOOLS_IN) + $(ZENSICAL_IN) -> $(ZENSICAL_LOCK) ..."
	$(DOCS_PIP_COMPILE) $(DOCS_PIP_COMPILE_FLAGS) -o $(ZENSICAL_LOCK) $(DOCS_TOOLS_IN) $(ZENSICAL_IN)
endif

.PHONY: docs-deps-upgrade
docs-deps-upgrade: docs-bootstrap ## Upgrade Zensical + dev tools deps to latest and lock
ifneq ($(DOCS_UV_MODE),)
	$(DOCS_UV_BIN) lock --project "$(DOCS_UV_PROJECT)" --upgrade
else
	@echo "Upgrading and compiling $(DOCS_TOOLS_IN) + $(ZENSICAL_IN) -> $(ZENSICAL_LOCK) ..."
	$(DOCS_PIP_COMPILE) --upgrade $(DOCS_PIP_COMPILE_FLAGS) -o $(ZENSICAL_LOCK) $(DOCS_TOOLS_IN) $(ZENSICAL_IN)
endif

.PHONY: docs-deps-outdated
docs-deps-outdated: docs-bootstrap $(if $(DOCS_UV_MODE),,$(ZENSICAL_LOCK)) ## Show diff of would-be Zensical upgrades
ifneq ($(DOCS_UV_MODE),)
	$(DOCS_UV_BIN) lock --project "$(DOCS_UV_PROJECT)" --upgrade --dry-run
else
	@tmp=$$(mktemp); \
	echo "Checking for available updates (dry-run) ..."; \
	$(DOCS_PIP_COMPILE) --upgrade $(DOCS_PIP_COMPILE_FLAGS) -o $$tmp $(DOCS_TOOLS_IN) $(ZENSICAL_IN) >/dev/null 2>&1 || true; \
	if ! diff -u $(ZENSICAL_LOCK) $$tmp >/dev/null 2>&1; then \
	  echo "Updates available for $(ZENSICAL_LOCK):"; \
	  diff -u $(ZENSICAL_LOCK) $$tmp || true; \
	  echo; echo "Run: make docs-deps-upgrade && make docs-deps-sync"; \
	else \
	  echo "Zensical requirements are up-to-date."; \
	fi; \
	rm -f $$tmp
endif

# The target's name before v1.16.0; kept so existing scripts and habits still work.
.PHONY: docs-deps-check-upgrade
docs-deps-check-upgrade: docs-deps-outdated

# ---- Sync + build / serve / audit / clean -----------------------------------

.PHONY: docs-deps-sync
docs-deps-sync: docs-bootstrap $(if $(DOCS_UV_MODE),,$(ZENSICAL_LOCK)) ## Sync venv to the Zensical + dev tools lock
ifneq ($(DOCS_UV_MODE),)
	$(DOCS_UV_BIN) sync --project "$(DOCS_UV_PROJECT)" --locked
else
	@echo "Syncing venv to $(ZENSICAL_LOCK) ..."
	$(DOCS_VENV_BIN)/pip-sync $(ZENSICAL_LOCK)
endif

.PHONY: docs-build
docs-build: docs-deps-sync ## Build the docs site (strict)
ifneq ($(DOCS_UV_MODE),)
	@echo "Building docs site..."
	$(DOCS_UV_RUN) zensical build -f $(ZENSICAL_CONFIG) --strict --clean
else
	@echo "Building docs site..."
	$(DOCS_VENV_BIN)/zensical build -f $(ZENSICAL_CONFIG) --strict --clean
endif

.PHONY: docs-serve
docs-serve: docs-deps-sync ## Serve the docs site with live reload
ifneq ($(DOCS_UV_MODE),)
	$(DOCS_UV_RUN) zensical serve -f $(ZENSICAL_CONFIG)$(if $(DOCS_ADDR), -a $(DOCS_ADDR))
else
	$(DOCS_VENV_BIN)/zensical serve -f $(ZENSICAL_CONFIG)$(if $(DOCS_ADDR), -a $(DOCS_ADDR))
endif

.PHONY: docs-audit
docs-audit: docs-deps-sync ## Audit docs dependencies for known vulnerabilities
ifneq ($(DOCS_UV_MODE),)
	@tmp=$$(mktemp); \
	status=0; \
	$(DOCS_UV_BIN) export --project "$(DOCS_UV_PROJECT)" --locked --no-emit-project --format requirements-txt -o $$tmp >/dev/null || status=$$?; \
	if [ $$status -eq 0 ]; then \
	  $(DOCS_UV_BIN) tool run pip-audit==$(DOCS_PIP_AUDIT_VERSION) --require-hashes --disable-pip -r $$tmp || true; \
	fi; \
	rm -f $$tmp; exit $$status
else
	$(DOCS_VENV_BIN)/pip-audit -r $(ZENSICAL_LOCK) || true
endif

.PHONY: docs-clean
docs-clean: ## Clean docs build artifacts
	rm -rf $(DOCS_SITE_DIR) .cache
ifneq ($(DOCS_UV_MODE),)
	@echo "To remove the environment as well: rm -rf $(DOCS_UV_PROJECT)/.venv"
else
	@echo "To remove the venv as well: rm -rf $(DOCS_PYTHON_VENV)"
endif

# ---- Convenience OS deps ----------------------------------------------------

.PHONY: docs-install-system-deps-ubuntu
docs-install-system-deps-ubuntu: ## Install Ubuntu system packages for the docs
	sudo apt-get update
	sudo apt-get install -y python3-venv python3-pip python3-full

.PHONY: docs-install-system-deps-macos
docs-install-system-deps-macos: ## Install macOS system packages for the docs
	@command -v brew >/dev/null 2>&1 || { echo "Homebrew not found: https://brew.sh"; exit 1; }
	brew install python || true

endif  # MK_COMMON_ZENSICAL_INCLUDED
