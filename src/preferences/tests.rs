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

use std::fs;
use std::process::Command;

use gtk::{gio, glib};
use tempfile::TempDir;

use super::*;

/// The schema id the tests compile the real schema under.
const SCHEMA_ID: &str = "com.leinardi.gx52.Test";

/// The app's real schema, compiled into a temporary directory, with settings on a memory backend
/// of their own: no test sees another's values or the user's.
fn test_settings() -> (TempDir, gio::Settings) {
    let dir = TempDir::new().expect("a temporary directory can be created");
    let source = include_str!("../../data/com.leinardi.gx52.gschema.xml.in")
        .replace("@app-id@", SCHEMA_ID)
        .replace("@app-path@", "/com/leinardi/gx52/Test/")
        .replace("@gettext-package@", "gx52");
    fs::write(dir.path().join(format!("{SCHEMA_ID}.gschema.xml")), source)
        .expect("the schema can be written");
    // Required by `meson setup` too, so it is there wherever the app builds.
    let status = Command::new("glib-compile-schemas")
        .arg("--strict")
        .arg(dir.path())
        .status()
        .expect("glib-compile-schemas runs");
    assert!(status.success(), "glib-compile-schemas failed: {status}");
    let source = gio::SettingsSchemaSource::from_directory(dir.path(), None, false)
        .expect("the compiled schema loads");
    let schema = source
        .lookup(SCHEMA_ID, false)
        .expect("the compiled schema has the app's schema");
    let backend = gio::memory_settings_backend_new();
    let settings = gio::Settings::new_full(&schema, Some(&backend), None);
    (dir, settings)
}

/// Another settings object on the same schema and backend: a writer outside the app, such as
/// `gsettings set` or another GX52 instance.
fn other_writer(settings: &gio::Settings) -> gio::Settings {
    let schema = settings
        .settings_schema()
        .expect("the settings have a schema");
    let backend: gio::SettingsBackend = settings.property("backend");
    gio::Settings::new_full(&schema, Some(&backend), None)
}

/// Runs `test` with its own thread-default main context, which delivers the settings' change
/// signals when [`drain`] iterates it.
fn with_context(test: impl FnOnce(&glib::MainContext)) {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| test(&context))
        .expect("the context is free");
}

/// Dispatches every pending change signal.
fn drain(context: &glib::MainContext) {
    while context.iteration(false) {}
}

fn set(settings: &gio::Settings, key: &str, value: bool) {
    settings
        .set_boolean(key, value)
        .expect("the key is writable");
}

#[test]
fn background_off_turns_autostart_off() {
    let (_dir, settings) = test_settings();
    set(&settings, AUTOSTART, true);
    set(&settings, RUN_IN_BACKGROUND, false);
    normalize_background(&settings);
    assert!(!settings.boolean(AUTOSTART));
}

#[test]
fn consistent_values_are_kept() {
    for (background, autostart) in [(true, true), (true, false), (false, false)] {
        let (_dir, settings) = test_settings();
        set(&settings, RUN_IN_BACKGROUND, background);
        set(&settings, AUTOSTART, autostart);
        normalize_background(&settings);
        assert_eq!(
            (
                settings.boolean(RUN_IN_BACKGROUND),
                settings.boolean(AUTOSTART)
            ),
            (background, autostart),
        );
    }
}

#[test]
fn following_fixes_stored_values() {
    with_context(|context| {
        let (_dir, settings) = test_settings();
        set(&settings, AUTOSTART, true);
        set(&settings, RUN_IN_BACKGROUND, false);
        follow_background(&settings);
        drain(context);
        assert!(!settings.boolean(AUTOSTART));
    });
}

#[test]
fn following_turns_autostart_off_when_background_goes_off() {
    with_context(|context| {
        let (_dir, settings) = test_settings();
        set(&settings, AUTOSTART, true);
        follow_background(&settings);
        drain(context);
        assert!(settings.boolean(AUTOSTART), "both on is consistent");

        // Written through another object, as `gsettings set` or the tray would.
        let other = other_writer(&settings);
        set(&other, RUN_IN_BACKGROUND, false);
        drain(context);
        assert!(!settings.boolean(AUTOSTART));

        // Turning it on again does not bring autostart back.
        set(&other, RUN_IN_BACKGROUND, true);
        drain(context);
        assert!(!settings.boolean(AUTOSTART));
    });
}

#[test]
fn following_turns_autostart_off_when_it_goes_on_without_background() {
    with_context(|context| {
        let (_dir, settings) = test_settings();
        set(&settings, RUN_IN_BACKGROUND, false);
        follow_background(&settings);
        drain(context);

        set(&other_writer(&settings), AUTOSTART, true);
        drain(context);
        assert!(!settings.boolean(AUTOSTART));
    });
}

#[test]
fn legacy_import_without_background_ends_without_autostart() {
    let (_dir, settings) = test_settings();
    apply_legacy(
        &settings,
        LegacySettings {
            launch_on_login: Some(true),
            run_in_background: Some(false),
        },
    );
    assert!(!settings.boolean(RUN_IN_BACKGROUND));
    assert!(!settings.boolean(AUTOSTART));
}

#[test]
fn legacy_import_keeps_what_it_does_not_name() {
    let (_dir, settings) = test_settings();
    apply_legacy(
        &settings,
        LegacySettings {
            launch_on_login: Some(true),
            run_in_background: None,
        },
    );
    assert!(settings.boolean(RUN_IN_BACKGROUND), "the default is on");
    assert!(settings.boolean(AUTOSTART));
}

#[test]
fn numeric_keys_have_the_s9_defaults_inside_their_bounds() {
    let (_dir, settings) = test_settings();
    let schema = settings
        .settings_schema()
        .expect("the settings have a schema");
    // docs/protocol.md, "Virtual mouse (uinput)".
    for (key, default, bounds) in [
        (VIRTUAL_MOUSE_SPEED, 0.6, (0.1, 3.0)),
        (VIRTUAL_MOUSE_CURVE, 1.6, (1.0, 3.0)),
        (VIRTUAL_MOUSE_DEADZONE, 1.0, (0.0, 4.0)),
    ] {
        let schema_key = schema.key(key);
        assert_eq!(spin_bounds(&schema_key), Some(bounds), "{key}");
        let value = number(&settings.value(key)).expect("the key is numeric");
        assert!((value - default).abs() < f64::EPSILON, "{key}: {value}");
        assert!((bounds.0..=bounds.1).contains(&value), "{key}: {value}");
    }
}

#[test]
fn keys_without_a_range_have_no_bounds() {
    let (_dir, settings) = test_settings();
    let schema = settings
        .settings_schema()
        .expect("the settings have a schema");
    assert_eq!(spin_bounds(&schema.key(VIRTUAL_MOUSE)), None);
}

#[test]
fn every_preference_is_in_the_schema() {
    let (_dir, settings) = test_settings();
    let schema = settings
        .settings_schema()
        .expect("the settings have a schema");
    for key in [
        RUN_IN_BACKGROUND,
        AUTOSTART,
        VIRTUAL_MOUSE,
        VIRTUAL_MOUSE_SPEED,
        VIRTUAL_MOUSE_CURVE,
        VIRTUAL_MOUSE_DEADZONE,
        VIRTUAL_MOUSE_INVERT_WHEEL,
    ] {
        assert!(schema.has_key(key), "{key}");
    }
    assert!(!settings.boolean(VIRTUAL_MOUSE), "off by default");
    assert!(!settings.boolean(VIRTUAL_MOUSE_INVERT_WHEEL));
}

mod value_object {
    use std::cell::Cell;

    use gtk::{glib, prelude::*, subclass::prelude::*};

    #[expect(
        clippy::min_ident_chars,
        reason = "the glib::Properties derive names its closure arguments with one letter"
    )]
    mod imp {
        use super::*;

        /// Stands in for a spin row: one double `value`, as `AdwSpinRow:value`.
        #[derive(Debug, Default, glib::Properties)]
        #[properties(wrapper_type = super::ValueObject)]
        pub struct ValueObject {
            #[property(get, set)]
            value: Cell<f64>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for ValueObject {
            const NAME: &'static str = "GX52TestValueObject";
            type Type = super::ValueObject;
        }

        #[glib::derived_properties]
        impl ObjectImpl for ValueObject {}
    }

    glib::wrapper! {
        pub struct ValueObject(ObjectSubclass<imp::ValueObject>);
    }

    impl Default for ValueObject {
        fn default() -> Self {
            glib::Object::new()
        }
    }
}

#[test]
fn numeric_keys_bind_to_a_double_property_both_ways() {
    use value_object::ValueObject;

    with_context(|context| {
        let (_dir, settings) = test_settings();
        for (key, from_key, from_property) in [
            (VIRTUAL_MOUSE_SPEED, 1.5, 2.2),
            (VIRTUAL_MOUSE_DEADZONE, 3.0, 2.0),
        ] {
            let object = ValueObject::default();
            settings.bind(key, &object, "value").build();
            drain(context);
            assert_eq!(
                Some(object.value()),
                number(&settings.value(key)),
                "{key}: the binding starts from the key"
            );

            settings
                .set_value(key, &variant_like(&settings.value(key), from_key))
                .expect("the key is writable");
            drain(context);
            assert!(
                (object.value() - from_key).abs() < f64::EPSILON,
                "{key}: {}",
                object.value()
            );

            object.set_value(from_property);
            drain(context);
            let stored = number(&settings.value(key)).expect("the key is numeric");
            assert!(
                (stored - from_property).abs() < f64::EPSILON,
                "{key}: {stored}"
            );
        }
    });
}

/// `value` in the type of `like` (a `d` or a `u` variant).
fn variant_like(like: &glib::Variant, value: f64) -> glib::Variant {
    if like.is::<u32>() {
        // The test values are whole and in range.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the test only passes small whole non-negative values"
        )]
        let whole = value as u32;
        whole.to_variant()
    } else {
        value.to_variant()
    }
}
