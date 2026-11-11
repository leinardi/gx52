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

//! The settings of the preferences dialog (docs/ui.md, "Preferences"): their keys, the rule that
//! starting on login needs running in the background, and the bounds of the numeric ones.

use gtk::{gio, glib, prelude::*};

use crate::profile::legacy::LegacySettings;

/// Keep GX52 running when its window is closed.
pub(crate) const RUN_IN_BACKGROUND: &str = "run-in-background";
/// Start GX52 hidden when logging in; only while [`RUN_IN_BACKGROUND`] is on.
pub(crate) const AUTOSTART: &str = "autostart";
/// Use the ministick as a mouse.
pub(crate) const VIRTUAL_MOUSE: &str = "virtual-mouse";
/// The virtual mouse speed, in pixels per ministick step and tick.
pub(crate) const VIRTUAL_MOUSE_SPEED: &str = "virtual-mouse-speed";
/// The exponent of the virtual mouse speed curve.
pub(crate) const VIRTUAL_MOUSE_CURVE: &str = "virtual-mouse-curve";
/// The ministick steps around the centre treated as centred.
pub(crate) const VIRTUAL_MOUSE_DEADZONE: &str = "virtual-mouse-deadzone";
/// Scroll the other way with the scroll wheel.
pub(crate) const VIRTUAL_MOUSE_INVERT_WHEEL: &str = "virtual-mouse-invert-wheel";

/// Turns [`AUTOSTART`] off while [`RUN_IN_BACKGROUND`] is off: starting hidden needs the app to
/// keep running without a window.
pub(crate) fn normalize_background(settings: &gio::Settings) {
    // Both keys are read every time, so both stay armed for `changed`.
    let autostart = settings.boolean(AUTOSTART);
    if !settings.boolean(RUN_IN_BACKGROUND)
        && autostart
        && let Err(error) = settings.set_boolean(AUTOSTART, false)
    {
        tracing::warn!("Cannot turn off {AUTOSTART}: {error}");
    }
}

/// Keeps [`AUTOSTART`] off while [`RUN_IN_BACKGROUND`] is off, from now on and for the values
/// already stored: whichever of the two keys a writer changes.
pub(crate) fn follow_background(settings: &gio::Settings) {
    for key in [RUN_IN_BACKGROUND, AUTOSTART] {
        settings.connect_changed(Some(key), |settings, _| normalize_background(settings));
    }
    // GSettings emits `changed` only for a key read since the handler was connected; this reads
    // both, and fixes values stored inconsistent by another writer while GX52 was not running.
    normalize_background(settings);
}

/// Writes the settings imported from GX52 0.7, then applies the background rule, whatever order
/// the change signals come in.
///
/// [`RUN_IN_BACKGROUND`] goes first: with [`follow_background`] connected, an imported autostart
/// written while the stored value is still off would be turned off at once and not come back.
pub(crate) fn apply_legacy(settings: &gio::Settings, legacy: LegacySettings) {
    for (key, value) in [
        (RUN_IN_BACKGROUND, legacy.run_in_background),
        (AUTOSTART, legacy.launch_on_login),
    ] {
        if let Some(value) = value
            && let Err(error) = settings.set_boolean(key, value)
        {
            tracing::warn!("Cannot import the GX52 0.7 setting {key}: {error}");
        }
    }
    normalize_background(settings);
}

/// The lower and upper bound of a numeric key with a range in the schema, as a spin row's
/// adjustment takes them; `None` for a key without one.
pub(crate) fn spin_bounds(key: &gio::SettingsSchemaKey) -> Option<(f64, f64)> {
    let range = key.range();
    if range.child_value(0).str()? != "range" {
        return None;
    }
    let bounds = range.child_value(1).as_variant()?;
    Some((
        number(&bounds.child_value(0))?,
        number(&bounds.child_value(1))?,
    ))
}

/// A `d`, `u` or `i` variant as a double.
fn number(value: &glib::Variant) -> Option<f64> {
    value
        .get::<f64>()
        .or_else(|| value.get::<u32>().map(f64::from))
        .or_else(|| value.get::<i32>().map(f64::from))
}

#[cfg(test)]
mod tests;
