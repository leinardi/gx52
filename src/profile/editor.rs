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

//! What the views of the profile editor show (docs/ui.md, "Profile editor").

use x52::device::DeviceType;
use x52::input::Mode;

use super::{ModeSettings, ProfileId, ProfileKind};

/// What the profile editor shows: the shown device type's active profile, in the mode shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorState {
    /// The profile.
    pub(crate) profile: ProfileId,
    /// The device type of the profile.
    pub(crate) device_type: DeviceType,
    /// Whether the profile has a set of settings per mode.
    pub(crate) kind: ProfileKind,
    /// The mode shown.
    pub(crate) mode: Mode,
    /// The settings of that mode.
    pub(crate) settings: ModeSettings,
}
