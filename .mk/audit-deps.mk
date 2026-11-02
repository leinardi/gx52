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
# Adapted from swarm-device-access/.mk/audit-deps.mk at 4cd31c4cea79, originally
# Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi.
#

ifndef MK_LOCAL_AUDIT_DEPS_INCLUDED
MK_LOCAL_AUDIT_DEPS_INCLUDED := 1

# Local snippet (NOT part of make-common): check every Rust dependency against the RustSec
# advisory database, with cargo-deny as configured in deny.toml. It is not reachability-aware: any
# advisory against a crate in the graph counts, which is why every ignore in deny.toml carries a
# reason. It needs the network for the database, so it runs when Cargo.toml, Cargo.lock or
# deny.toml change (the cargo-deny hook, with every check), on every pull request and every
# release, and deliberately here. CARGO comes from rust.mk; rust-deny there runs every cargo-deny
# check, this only the advisories.

.PHONY: audit-deps
audit-deps: ## Check every Rust dependency against the RustSec advisories (cargo-deny; network required)
	$(CARGO) deny --workspace check advisories

endif  # MK_LOCAL_AUDIT_DEPS_INCLUDED
