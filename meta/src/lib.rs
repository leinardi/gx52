// Copyright 2026 Roberto Leinardi.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//
// Adapted from swarm-device-access/meta/src/lib.rs at 4cd31c4, originally Apache-2.0-licensed,
// Copyright (c) 2026 Roberto Leinardi

//! Repository meta-tests: checks on the repository itself rather than on the application. The
//! tests live under `tests/`; this library holds what they share: the repository's files, an
//! isolated git environment, and the Markdown reader. Every meta-test fails rather than skips when
//! something it needs is missing, and carries a negative control: a case proving the rule fails on
//! the input it exists to stop.

pub mod gitenv;
pub mod links;
pub mod repo;
