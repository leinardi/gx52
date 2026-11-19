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
// save_png is ported from the S7 spike, spikes/s7-led-editor/src/main.rs (save_window_png) on the
// spikes branch at 345116e.

//! The screenshots themselves: the shot table, and the GTK side of the [`Driver`]: it sets each
//! shot up, polls its readiness, waits for frames and saves the window as a PNG.

mod driver;

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use x52::device::DeviceType;
use x52::input::{self as x52_input, Axis, Button, Mode};

use self::driver::{Action, Driver, Input, ShotPlan};
use super::{ACTIVE_PROFILE, NARROW, WIDE};
use crate::application::Application;
use crate::device::access::AccessState;
use crate::device::monotonic::{Monotonic, SystemMonotonic};
use crate::profile::active;
use crate::widgets::PreferencesDialog;
use crate::window::Window;

/// How long a shot may take to become ready.
const SHOT_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the whole run may take.
const RUN_DEADLINE: Duration = Duration::from_secs(120);

/// How often a shot is polled, and a frame wait reported idle.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The fixture text on the first MFD line of the active profile in mode 1.
const MFD_FIRST_LINE: &str = "MASTER ARM ON";

/// The stick's left-to-right value in the input tester shot's pose (the screenshot backend).
const POSE_STICK_X: i32 = 700;

/// The time the MFD shows: the fixed instant in the fixed local zone.
const MFD_TIME: &str = "12:09";

/// Whether a condition of a shot holds; the error names the first one that does not.
type Readiness = Result<(), &'static str>;

/// One screenshot.
struct Shot {
    /// The PNG's name without the extension, as the metainfo names it.
    name: &'static str,
    /// Brings the window to the shot.
    set_up: fn(&Application, &Window),
    /// Whether the window shows the shot completely.
    ready: fn(&Application, &Window) -> Readiness,
}

/// The screenshots, in the order they are taken.
const SHOTS: [Shot; 6] = [
    Shot {
        name: "leds",
        set_up: |_, window| show_view(window, "leds", WIDE),
        ready: |app, window| {
            device_ready(app)?;
            sized(window, WIDE)?;
            check(
                window.imp().led_editor.is_mapped(),
                "the LED editor is mapped",
            )
        },
    },
    Shot {
        name: "mfd",
        set_up: |_, window| show_view(window, "mfd", WIDE),
        ready: |app, window| {
            device_ready(app)?;
            sized(window, WIDE)?;
            let editor = &window.imp().mfd_editor;
            check(editor.is_mapped(), "the MFD editor is mapped")?;
            let (lines, time) = editor.display().shown_text();
            check(
                lines
                    .first()
                    .is_some_and(|line| line.trim_end() == MFD_FIRST_LINE),
                "the MFD preview shows the fixture text",
            )?;
            check(time == MFD_TIME, "the MFD preview shows the fixed time")
        },
    },
    Shot {
        name: "clocks",
        set_up: |_, window| show_view(window, "clocks", WIDE),
        ready: |app, window| {
            device_ready(app)?;
            sized(window, WIDE)?;
            let editor = &window.imp().clock_editor;
            check(editor.is_mapped(), "the clock editor is mapped")?;
            check(editor.times_shown(), "every clock shows its time")
        },
    },
    Shot {
        name: "profiles",
        set_up: |_, window| {
            resize(window, NARROW);
            window.imp().split_view.set_show_content(false);
        },
        ready: |app, window| {
            device_ready(app)?;
            sized(window, NARROW)?;
            let imp = window.imp();
            check(
                imp.split_view.is_collapsed() && !imp.split_view.shows_content(),
                "the narrow window shows the sidebar",
            )?;
            check(imp.profile_list.is_mapped(), "the profile list is mapped")?;
            check(
                imp.profile_list.row_at_index(3).is_some()
                    && imp.profile_list.row_at_index(4).is_none(),
                "the profile list has the four fixture profiles",
            )
        },
    },
    Shot {
        name: "preferences",
        set_up: |app, window| {
            resize(window, WIDE);
            window.imp().split_view.set_show_content(true);
            app.activate_action("preferences", None);
        },
        ready: |app, window| {
            device_ready(app)?;
            sized(window, WIDE)?;
            let dialog = app.preferences_dialog();
            check(
                dialog.as_ref().is_some_and(WidgetExt::is_mapped),
                "the preferences dialog is mapped",
            )?;
            check(
                dialog
                    .as_ref()
                    .is_some_and(|dialog| fully_open(dialog, window)),
                "the preferences dialog has finished opening",
            )?;
            check(
                dialog.is_some_and(|dialog| dialog.virtual_mouse_active()),
                "the virtual mouse switch is on",
            )
        },
    },
    // Last: the pose it puts the joystick in stays for the rest of the run.
    Shot {
        name: "tester",
        set_up: |app, window| {
            if let Some(dialog) = app.preferences_dialog() {
                dialog.force_close();
            }
            super::backend::show_pose();
            resize(window, WIDE);
            window.open_tester();
            window.imp().split_view.set_show_content(true);
        },
        ready: |app, window| {
            device_ready(app)?;
            sized(window, WIDE)?;
            check(
                app.preferences_dialog()
                    .is_none_or(|dialog| !dialog.is_mapped()),
                "the preferences dialog is closed",
            )?;
            check(
                app.held_inputs()
                    .is_held(x52_input::Input::Button(Button::TriggerStage1)),
                "the pose's buttons are held",
            )?;
            check(
                app.axis_values()
                    .get(Axis::StickX)
                    .is_some_and(|state| state.value == POSE_STICK_X),
                "the pose's axes are read",
            )?;
            check(
                window.imp().input_tester.shows_current(app),
                "the input tester shows the pose",
            )
        },
    },
];

/// The names of the shots, in order: the PNG file names without the extension.
#[cfg(test)]
pub(crate) fn shot_names() -> impl Iterator<Item = &'static str> {
    SHOTS.iter().map(|shot| shot.name)
}

/// `Ok` when `condition` holds, else `name`.
fn check(condition: bool, name: &'static str) -> Readiness {
    if condition { Ok(()) } else { Err(name) }
}

/// What every shot needs: the fixture profile active, and the X52 Pro open in mode 1.
fn device_ready(app: &Application) -> Readiness {
    let status = app.device_status();
    check(
        status.device_type == Some(DeviceType::X52Pro) && status.access == AccessState::Ready,
        "the X52 Pro is open",
    )?;
    check(
        app.selector_mode() == Some(Mode::One),
        "the mode selector is in mode 1",
    )?;
    let active = app
        .imp()
        .settings
        .get()
        .map(|settings| settings.string(active::settings_key(DeviceType::X52Pro)));
    check(
        active.is_some_and(|id| id == ACTIVE_PROFILE),
        "the fixture profile is active",
    )?;
    check(
        super::active_profile().is_some_and(|id| app.profile(&id).is_some()),
        "the fixture profiles are loaded",
    )
}

/// Whether `dialog` is drawn at its full size in `window`: its opening animation, which runs even
/// with the animations off, scales it up from less.
fn fully_open(dialog: &PreferencesDialog, window: &Window) -> bool {
    let Some(child) = dialog.child() else {
        return false;
    };
    let Some(bounds) = child.compute_bounds(window) else {
        return false;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "widget sizes are far below f32's exact integer range"
    )]
    let size = (child.width() as f32, child.height() as f32);
    child.width() > 0 && (bounds.width(), bounds.height()) == size
}

/// Whether the window has the shot's size.
fn sized(window: &Window, (width, height): (i32, i32)) -> Readiness {
    check(
        window.width() == width && window.height() == height,
        "the window has the shot's size",
    )
}

/// Resizes the window to `size`.
fn resize(window: &Window, (width, height): (i32, i32)) {
    window.set_default_size(width, height);
}

/// Shows the view `name` in a window of `size`, with the content page shown.
fn show_view(window: &Window, name: &str, size: (i32, i32)) {
    resize(window, size);
    window.imp().split_view.set_show_content(true);
    if let Err(error) =
        WidgetExt::activate_action(window, "win.show-view", Some(&name.to_variant()))
    {
        tracing::error!("Cannot show the {name} view: {error}");
    }
}

/// What the run waits for next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    /// The timer polls shot `index`.
    Poll(usize),
    /// A frame-clock tick, or the timer.
    Frame,
    /// Nothing: the run is over.
    Finished,
}

/// The running screenshot run.
struct Runner {
    driver: RefCell<Driver>,
    waiting: Cell<Waiting>,
    clock: SystemMonotonic,
    app: glib::WeakRef<Application>,
    window: glib::WeakRef<Window>,
}

/// Starts the run in `window` of `app`: when it ends, the application quits.
pub(crate) fn start(app: &Application, window: &Window) {
    let plans = SHOTS
        .iter()
        .map(|shot| ShotPlan {
            name: shot.name,
            timeout: SHOT_TIMEOUT,
        })
        .collect();
    let runner = Rc::new(Runner {
        driver: RefCell::new(Driver::new(plans, RUN_DEADLINE)),
        waiting: Cell::new(Waiting::Finished),
        clock: SystemMonotonic::new(),
        app: app.downgrade(),
        window: window.downgrade(),
    });
    runner.feed(Input::Start);

    let timer = Rc::clone(&runner);
    glib::timeout_add_local(POLL_INTERVAL, move || timer.on_timer());
    window.add_tick_callback(move |_, _| runner.on_frame());
}

impl Runner {
    /// Polls the shot, or reports a frame wait idle.
    fn on_timer(&self) -> glib::ControlFlow {
        match self.waiting.get() {
            Waiting::Poll(index) => {
                let (Some(app), Some(window)) = (self.app.upgrade(), self.window.upgrade()) else {
                    return glib::ControlFlow::Break;
                };
                let readiness = (SHOTS[index].ready)(&app, &window);
                self.feed(Input::Polled(readiness));
            }
            Waiting::Frame => self.feed(Input::Idle),
            Waiting::Finished => return glib::ControlFlow::Break,
        }
        glib::ControlFlow::Continue
    }

    /// Counts a frame the run waits for.
    fn on_frame(&self) -> glib::ControlFlow {
        match self.waiting.get() {
            Waiting::Frame => self.feed(Input::Frame),
            Waiting::Poll(_) => {}
            Waiting::Finished => return glib::ControlFlow::Break,
        }
        glib::ControlFlow::Continue
    }

    /// Passes `input` to the driver and carries its actions out.
    fn feed(&self, input: Input) {
        let (Some(app), Some(window)) = (self.app.upgrade(), self.window.upgrade()) else {
            self.waiting.set(Waiting::Finished);
            return;
        };
        let mut input = input;
        loop {
            let action = self.driver.borrow_mut().step(input, self.clock.elapsed());
            match action {
                Action::SetUp(index) => {
                    tracing::info!("Screenshots: setting up {}", SHOTS[index].name);
                    (SHOTS[index].set_up)(&app, &window);
                    self.waiting.set(Waiting::Poll(index));
                }
                Action::Poll(index) => self.waiting.set(Waiting::Poll(index)),
                Action::WaitFrame => self.waiting.set(Waiting::Frame),
                Action::Capture(index) => match capture(&window, SHOTS[index].name) {
                    Ok(()) => {
                        input = Input::Captured;
                        continue;
                    }
                    Err(message) => {
                        self.waiting.set(Waiting::Finished);
                        super::fail(&app, &message);
                    }
                },
                Action::Done => {
                    self.waiting.set(Waiting::Finished);
                    tracing::info!("Screenshots: done");
                    app.quit();
                }
                Action::Fail { shot, condition } => {
                    self.waiting.set(Waiting::Finished);
                    super::fail(
                        &app,
                        &format!("shot {shot} was never ready: not met: {condition}"),
                    );
                }
            }
            break;
        }
    }
}

/// Saves `window` as `<name>.png` in the output directory.
fn capture(window: &Window, name: &str) -> Result<(), String> {
    let scale = window.scale_factor();
    if scale != 1 {
        return Err(format!(
            "shot {name}: the window's scale factor is {scale}, not 1"
        ));
    }
    let Some(dir) = super::out_dir() else {
        return Err(format!("shot {name}: no output directory"));
    };
    let path = dir.join(format!("{name}.png"));
    save_png(window, &path).map_err(|error| format!("shot {name}: {error}"))?;
    tracing::info!("Screenshots: saved {}", path.display());
    Ok(())
}

/// The window's corner radius, libadwaita's `--window-radius`: the headless compositor shows the
/// window tiled, with square corners, so the shot rounds them itself.
const CORNER_RADIUS: f32 = 15.0;

/// The transparent border around the window that holds its shadow.
const SHADOW_MARGIN: f32 = 32.0;

/// libadwaita's `window.csd` box-shadow, outermost first: (opacity, spread, blur radius). Flathub's
/// quality guidelines ask for the window's shadow and rounded corners in the screenshots.
const SHADOW: [(f32, f32, f32); 3] = [(0.15, 5.0, 14.0), (0.10, 2.0, 5.0), (0.05, 1.0, 0.0)];

/// Renders `window` as it is now, with rounded corners and libadwaita's window shadow on a
/// transparent margin, and saves it to `path`.
fn save_png(window: &Window, path: &Path) -> Result<(), String> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a window is a few thousand pixels at most, exact in an f32"
    )]
    let (width, height) = (window.width() as f32, window.height() as f32);
    let frame = gsk::RoundedRect::from_rect(
        graphene::Rect::new(SHADOW_MARGIN, SHADOW_MARGIN, width, height),
        CORNER_RADIUS,
    );
    let snapshot = gtk::Snapshot::new();
    for (opacity, spread, blur) in SHADOW {
        snapshot.append_outset_shadow(
            &frame,
            &gdk::RGBA::new(0.0, 0.0, 0.0, opacity),
            0.0,
            0.0,
            spread,
            blur,
        );
    }
    snapshot.push_rounded_clip(&frame);
    snapshot.save();
    snapshot.translate(&graphene::Point::new(SHADOW_MARGIN, SHADOW_MARGIN));
    gtk::WidgetPaintable::new(Some(window)).snapshot(
        &snapshot,
        f64::from(width),
        f64::from(height),
    );
    snapshot.restore();
    snapshot.pop();
    let node = snapshot
        .to_node()
        .ok_or_else(|| "the window drew nothing".to_owned())?;
    let renderer = window
        .renderer()
        .ok_or_else(|| "the window has no renderer".to_owned())?;
    let viewport = graphene::Rect::new(
        0.0,
        0.0,
        width + 2.0 * SHADOW_MARGIN,
        height + 2.0 * SHADOW_MARGIN,
    );
    let texture: gdk::Texture = renderer.render_texture(node, Some(&viewport));
    texture
        .save_to_png(path)
        .map_err(|error| format!("cannot save {}: {error}", path.display()))
}
