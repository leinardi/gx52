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
# Adapted from swarm-device-access/.mk/rust.mk at 4cd31c4cea79, originally Apache-2.0-licensed,
# Copyright (c) 2026 Roberto Leinardi.
#

ifndef MK_LOCAL_RUST_INCLUDED
MK_LOCAL_RUST_INCLUDED := 1

# Local snippet (NOT part of make-common): the Cargo workspace (x52, gx52, meta). Every lint and
# test command runs workspace-wide, so every member is held to the same lints and tests: the lint
# set lives once in the root [workspace.lints], and a command scoped to one package would leave the
# others unchecked. --locked everywhere: Cargo.lock is the pin, and a command that rewrote it would
# test a dependency graph nobody reviewed.
#
# These targets drive cargo directly, for the fast edit-compile-lint loop. They need the GTK 4 and
# libadwaita development files on the host; the application itself is built by Meson (meson.mk),
# which generates src/config.rs and the GResource bundle before invoking cargo.

CARGO ?= cargo

.PHONY: rust-build
rust-build: ## Build the Rust workspace with cargo (Meson builds the installable app)
	$(CARGO) build --workspace --all-targets --all-features --locked

.PHONY: rust-test
rust-test: ## Run the Rust tests, meta-tests included
	$(CARGO) test --workspace --locked

.PHONY: rust-clippy
rust-clippy: ## Lint the Rust workspace with clippy, warnings as errors
	$(CARGO) clippy --workspace --all-targets --all-features --locked -- -D warnings

.PHONY: rust-fmt
rust-fmt: ## Format the Rust code
	$(CARGO) fmt --all

.PHONY: rust-fmt-check
rust-fmt-check: ## Fail if the Rust code needs formatting
	$(CARGO) fmt --all --check

.PHONY: rust-doc
rust-doc: ## Check Rust documentation, warnings as errors
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" $(CARGO) doc --workspace --all-features --no-deps --locked

.PHONY: rust-deny
rust-deny: ## Check Rust dependencies with cargo-deny (advisories, licences, bans, sources; network required)
	$(CARGO) deny --workspace check

# rust-deny stays out of verify-rust: its advisories need the network, as audit-deps does.
.PHONY: verify-rust
verify-rust: rust-build rust-fmt-check rust-clippy rust-test rust-doc ## Verify the Rust code (build, fmt, clippy, tests, docs)

endif  # MK_LOCAL_RUST_INCLUDED
