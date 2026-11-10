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

//! The widgets of the main window.

mod device_footer;
mod led_button;
mod led_editor;
mod mode_bar;
mod profile_row;

use gtk::prelude::*;

pub(crate) use self::device_footer::DeviceFooter;
pub(crate) use self::led_button::LedButton;
pub(crate) use self::led_editor::LedEditor;
pub(crate) use self::mode_bar::ModeBar;
pub(crate) use self::profile_row::{ProfileRow, RowAction};
use crate::application::Application;
use crate::window::Window;

/// The application of the window `widget` is in.
fn app_of(widget: &impl IsA<gtk::Widget>) -> Option<Application> {
    widget.root().and_downcast::<Window>()?.app()
}
