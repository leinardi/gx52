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

mod clock_editor;
mod device_footer;
mod led_button;
mod led_editor;
mod mfd_display;
mod mfd_editor;
mod mode_bar;
mod preferences_dialog;
mod profile_row;
mod ticker;
mod udev_guide;
mod uinput_page;
mod zone_clock_group;
mod zone_dialog;

use gtk::prelude::*;
use x52::command::Brightness;

pub(crate) use self::clock_editor::ClockEditor;
pub(crate) use self::device_footer::DeviceFooter;
pub(crate) use self::led_button::LedButton;
pub(crate) use self::led_editor::LedEditor;
pub(crate) use self::mfd_display::MfdDisplay;
pub(crate) use self::mfd_editor::MfdEditor;
pub(crate) use self::mode_bar::ModeBar;
pub(crate) use self::preferences_dialog::PreferencesDialog;
pub(crate) use self::profile_row::{ProfileRow, RowAction};
pub(crate) use self::udev_guide::UdevGuide;
pub(crate) use self::uinput_page::UinputPage;
pub(crate) use self::zone_clock_group::ZoneClockGroup;
pub(crate) use self::zone_dialog::ZoneDialog;
use crate::application::Application;
use crate::window::Window;

/// The application of the window `widget` is in.
fn app_of(widget: &impl IsA<gtk::Widget>) -> Option<Application> {
    widget.root().and_downcast::<Window>()?.app()
}

/// The level of a brightness scale at `value`.
fn brightness_at(value: f64) -> Option<Brightness> {
    let value = value.round();
    // The scales are bounded to 0–128: the level whose value it is.
    let level = (0..=Brightness::MAX).find(|level| f64::from(*level) >= value)?;
    Brightness::new(level).ok()
}
