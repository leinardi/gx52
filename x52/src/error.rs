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

//! The error of the checked constructors of the command values.

/// A value that a command cannot carry.
///
/// Returned by every checked constructor in [`crate::command`], so a value outside the ranges of
/// docs/protocol.md ("Commands the app may send") never becomes a command.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RangeError {
    /// A number outside its allowed range.
    #[error("{field} {value} is outside the allowed range {min}..={max}")]
    OutOfRange {
        /// What the value is, for example `brightness`.
        field: &'static str,
        /// The value given.
        value: i64,
        /// The smallest allowed value.
        min: i64,
        /// The largest allowed value.
        max: i64,
    },
    /// A byte that the MFD shows as blank, which is never written: the only blank a line may
    /// contain is the space, `0x20`.
    #[error("byte {byte:#04x} at position {position} is a blank of the MFD character set")]
    BlankByte {
        /// The byte given.
        byte: u8,
        /// Its position in the line.
        position: usize,
    },
}

impl RangeError {
    /// A value of `field` outside `min..=max`.
    pub(crate) fn out_of_range(
        field: &'static str,
        value: impl Into<i64>,
        min: impl Into<i64>,
        max: impl Into<i64>,
    ) -> Self {
        Self::OutOfRange {
            field,
            value: value.into(),
            min: min.into(),
            max: max.into(),
        }
    }
}
