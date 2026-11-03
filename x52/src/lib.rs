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

//! The X52 and X52 Pro protocol, device model, key maps and MFD character set.
//!
//! No GTK dependency: everything here is testable without a display or a device. The commands of
//! [`command`] are the only ones that can be built, and [`transport::send`] is the only way to
//! send one.

pub mod charset;
pub mod clock;
pub mod command;
pub mod device;
pub mod error;
pub mod input;
pub mod transport;

#[cfg(test)]
mod test_support;
