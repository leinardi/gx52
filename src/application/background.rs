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

//! Running in the background (docs/architecture.md, "ksni tray"): with "Run in Background" on,
//! closing the window destroys it and a hold keeps the application, the device service and the
//! tray running; showing the window builds a new one. Quit ends it for good, once the profiles are
//! written.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use super::Application;
use crate::device::watch;
use crate::preferences::RUN_IN_BACKGROUND;
use crate::profile::list;
use crate::profile::order;
use crate::tray::{
    self, Gx52Tray, ProfileChoices, TrayAction, TrayGeneration, TrayHandle, TrayMessage, TrayModel,
    WatcherReport,
};
use crate::window::Window;

/// What closing the window does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAction {
    /// The application keeps running without a window.
    Background,
    /// The application ends once the profiles are written.
    Quit,
}

/// What closing the window does with "Run in Background" set to `run_in_background`, while the
/// user `quitting` or not.
fn close_action(run_in_background: bool, quitting: bool) -> CloseAction {
    if run_in_background && !quitting {
        CloseAction::Background
    } else {
        CloseAction::Quit
    }
}

/// Whether a watcher report brings the window back: the tray host that showed the icon went away
/// while no window is there, so nothing would lead back to it. Never while quitting, which would
/// keep the application from ending.
fn recovers_window(report: WatcherReport, window_exists: bool, quitting: bool) -> bool {
    report == WatcherReport::Lost && !window_exists && !quitting
}

/// The dialogs shown over a window, top first.
trait DialogStack {
    type Dialog: PartialEq;

    /// The dialog on top, if any.
    fn top_dialog(&self) -> Option<Self::Dialog>;

    /// Closes `dialog` without asking it.
    fn force_close(&self, dialog: &Self::Dialog);
}

impl DialogStack for adw::ApplicationWindow {
    type Dialog = adw::Dialog;

    fn top_dialog(&self) -> Option<adw::Dialog> {
        self.visible_dialog()
    }

    fn force_close(&self, dialog: &adw::Dialog) {
        dialog.force_close();
    }
}

/// Closes every dialog of `stack`, top first. A dialog still reported after its close (an
/// animation) ends the loop rather than looping on it.
fn close_dialogs(stack: &impl DialogStack) {
    let mut closed = None;
    while let Some(dialog) = stack.top_dialog() {
        if closed.as_ref() == Some(&dialog) {
            break;
        }
        stack.force_close(&dialog);
        closed = Some(dialog);
    }
}

/// Closes `window` by the same path as its close button, dialogs included: with a dialog open,
/// closing the window closes only that dialog, so "Hide GX52" and Quit would leave it shown.
fn close_window(window: &gtk::Window) {
    if let Some(window) = window.downcast_ref::<adw::ApplicationWindow>() {
        close_dialogs(window);
    }
    window.close();
}

/// Whether `action` shows the window, with `window_exists` or not: such a click's activation token
/// goes to the window, every other click's ends its launch.
fn shows_window(action: &TrayAction, window_exists: bool) -> bool {
    match action {
        TrayAction::ToggleWindow => !window_exists,
        TrayAction::Preferences | TrayAction::About => true,
        TrayAction::ActivateProfile(_)
        | TrayAction::Quit
        | TrayAction::Registered
        | TrayAction::WatcherOnline
        | TrayAction::WatcherOffline => false,
    }
}

/// Whether a change of "Run in Background" shows the window: turned off while no window is there
/// and GX52 is not quitting, the tray goes with it and the window is the only way back.
fn shows_window_without_background(
    run_in_background: bool,
    quitting: bool,
    window_exists: bool,
) -> bool {
    !run_in_background && !quitting && !window_exists
}

impl Application {
    /// Follows "Run in Background", the windows and the tray's clicks; starts the tray when it is
    /// on. Called once, in `startup`, after the settings.
    pub(super) fn setup_background(&self) {
        let (sender, receiver) = async_channel::unbounded();
        // Set once, here.
        let _ = self.imp().tray_actions.set(sender);
        // The application owns the sender, so the loop never ends on its own: it holds the
        // application only while handling a message.
        let app = self.downgrade();
        glib::spawn_future_local(async move {
            while let Ok(message) = receiver.recv().await {
                let Some(app) = app.upgrade() else {
                    break;
                };
                app.on_tray_message(message);
            }
        });
        if let Some(settings) = self.imp().settings.get() {
            settings.connect_changed(
                Some(RUN_IN_BACKGROUND),
                glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    move |_, _| app.on_run_in_background_changed()
                ),
            );
        }
        // The Show / Hide label follows the window.
        self.connect_window_added(|app, _| app.update_tray());
        self.connect_window_removed(|app, _| app.update_tray());
        // Reads the key, which arms its `changed` signal.
        self.sync_tray();
    }

    /// Whether "Run in Background" is on.
    fn runs_in_background(&self) -> bool {
        self.imp()
            .settings
            .get()
            .is_some_and(|settings| settings.boolean(RUN_IN_BACKGROUND))
    }

    /// The main window, if there is one.
    fn main_window(&self) -> Option<Window> {
        self.windows()
            .into_iter()
            .find_map(|window| window.downcast::<Window>().ok())
    }

    /// The window is closing: keeps the application running without it when it runs in the
    /// background.
    pub(crate) fn window_closing(&self) {
        let imp = self.imp();
        match close_action(self.runs_in_background(), imp.quitting.get()) {
            CloseAction::Background => {
                if imp.background_hold.borrow().is_none() {
                    tracing::info!("Window closed: running in the background");
                    imp.background_hold.replace(Some(self.hold()));
                }
            }
            CloseAction::Quit => {}
        }
    }

    /// Quit: the tray goes, the window closes and the application ends once the profiles are
    /// written.
    pub(super) fn quit_for_good(&self) {
        let imp = self.imp();
        imp.quitting.set(true);
        self.sync_tray();
        // Close the windows instead of calling quit(): quit() skips close-request, so windows
        // could not save their state. The application exits by itself once its last window is
        // gone and the holds are released.
        for window in self.windows() {
            close_window(&window);
        }
        imp.background_hold.take();
    }

    /// The user brought GX52 back (a launch, the tray): a quit in progress is called off and the
    /// window no longer needs the background hold. Called from `activate`, once a window is
    /// presented.
    pub(super) fn back_from_background(&self) {
        let imp = self.imp();
        imp.quitting.set(false);
        imp.background_hold.take();
        self.sync_tray();
    }

    fn on_run_in_background_changed(&self) {
        self.sync_tray();
        if shows_window_without_background(
            self.runs_in_background(),
            self.imp().quitting.get(),
            self.main_window().is_some(),
        ) {
            self.activate();
        }
    }

    /// Starts the tray while "Run in Background" is on and GX52 is not quitting, and stops it
    /// otherwise.
    fn sync_tray(&self) {
        let imp = self.imp();
        if self.runs_in_background() && !imp.quitting.get() {
            let generation = imp.tray.borrow_mut().enable();
            if let Some(generation) = generation {
                self.spawn_tray(generation);
            }
        } else {
            let handle = imp.tray.borrow_mut().disable();
            if let Some(handle) = handle {
                tracing::info!("Tray stopped");
                handle.shut_down();
            }
        }
    }

    /// Stops the tray for the application's shutdown.
    pub(super) fn stop_tray(&self) {
        let handle = self.imp().tray.borrow_mut().disable();
        if let Some(handle) = handle {
            handle.shut_down();
        }
    }

    fn spawn_tray(&self, generation: TrayGeneration) {
        let imp = self.imp();
        let Some(actions) = imp.tray_actions.get() else {
            return;
        };
        imp.tray_model.publish(self.tray_model());
        imp.tray_published.replace(None);
        let tray = Gx52Tray::new(generation, imp.tray_model.clone(), actions.clone());
        let actions = actions.clone();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let spawned = tray.spawn(watch::is_sandboxed()).await;
                let registered = spawned.is_ok();
                app.on_tray_spawned(generation, spawned);
                if registered {
                    // Through the channel the tray's own reports take, so it comes after the
                    // missing-host report the start may have sent.
                    let message = TrayMessage {
                        generation,
                        action: TrayAction::Registered,
                        activation_token: None,
                    };
                    if let Err(error) = actions.try_send(message) {
                        tracing::debug!("Dropped the tray registration: {error}");
                    }
                }
            }
        ));
    }

    fn on_tray_spawned(
        &self,
        generation: TrayGeneration,
        spawned: Result<TrayHandle, ksni::Error>,
    ) {
        let imp = self.imp();
        match spawned {
            Ok(handle) => {
                let obsolete = imp.tray.borrow_mut().spawned(generation, handle);
                if let Some(obsolete) = obsolete {
                    tracing::debug!("Tray {generation} started after it was turned off");
                    obsolete.shut_down();
                } else {
                    tracing::info!("Tray started");
                    // The model may have changed while it started, with nothing to tell the host.
                    imp.tray_published.replace(None);
                    self.update_tray();
                }
            }
            Err(error) => {
                if imp.tray.borrow_mut().spawn_failed(generation) {
                    tracing::warn!("Cannot show the tray icon: {error}");
                }
            }
        }
    }

    /// What the tray shows now.
    fn tray_model(&self) -> TrayModel {
        let status = self.device_status();
        let active = status
            .device_type
            .and_then(|device_type| self.active_profile_id(device_type))
            .and_then(|id| self.profile(&id).map(|profile| (id, profile)));
        let line = status.tray_line(
            self.selector_mode(),
            active
                .as_ref()
                .map(|(_, profile)| (profile.name.as_str(), profile.kind())),
        );
        TrayModel {
            status: line,
            window_shown: self.main_window().is_some(),
            profiles: status.device_type.and_then(|device_type| {
                let imp = self.imp();
                let profiles = imp.profiles.borrow();
                let store = profiles.as_ref()?;
                let entries = imp.settings.get()?.strv(order::settings_key(device_type));
                let entries: Vec<&str> = entries.iter().map(glib::GStringPtr::as_str).collect();
                let rows = list::rows(store, device_type, &entries);
                Some(ProfileChoices::from_rows(
                    rows,
                    active.as_ref().map(|(id, _)| id),
                ))
            }),
        }
    }

    /// Shows the current state in the tray, if it changed.
    pub(super) fn update_tray(&self) {
        let imp = self.imp();
        // Runs on every edit: nothing to build while there is no tray. A start publishes the
        // model it starts with.
        if imp.tray.borrow().is_off() {
            return;
        }
        let model = self.tray_model();
        if imp.tray_published.borrow().as_ref() == Some(&model) {
            return;
        }
        imp.tray_model.publish(model.clone());
        imp.tray_published.replace(Some(model));
        let handle = imp.tray.borrow().handle().cloned();
        if let Some(handle) = handle {
            glib::spawn_future_local(async move { handle.refresh().await });
        }
    }

    /// Shows the window, with the activation token of the click that asked for it, so the
    /// compositor ends that click's launch and gives the window the focus.
    fn present_window(&self, activation_token: Option<String>) {
        self.imp().startup_id.replace(activation_token);
        self.activate();
    }

    fn on_tray_message(&self, message: TrayMessage) {
        let imp = self.imp();
        let TrayMessage {
            generation,
            action,
            activation_token,
        } = message;
        let window = self.main_window();
        let current = imp.tray.borrow().is_current(generation) && !imp.quitting.get();
        if !current || !shows_window(&action, window.is_some()) {
            // Every click's launch ends, a dropped one's too: the busy cursor stays otherwise.
            if let Some(token) = &activation_token {
                tray::launch::end(token);
            }
        }
        if !current {
            tracing::debug!("Dropped an action of tray {generation}: {action:?}");
            return;
        }
        match action {
            TrayAction::ToggleWindow => match window {
                Some(window) => close_window(window.upcast_ref()),
                None => self.present_window(activation_token),
            },
            TrayAction::ActivateProfile(id) => self.activate_profile(&id),
            TrayAction::Preferences => {
                self.present_window(activation_token);
                self.show_preferences_dialog();
            }
            TrayAction::About => {
                self.present_window(activation_token);
                self.show_about_dialog();
            }
            TrayAction::Quit => self.quit_for_good(),
            TrayAction::Registered => {
                imp.tray.borrow_mut().registered(generation);
            }
            TrayAction::WatcherOnline => {
                imp.tray.borrow_mut().watcher_online(generation);
                tracing::info!("Tray host available");
            }
            TrayAction::WatcherOffline => {
                let report = imp.tray.borrow_mut().watcher_offline(generation);
                if recovers_window(report, self.main_window().is_some(), imp.quitting.get()) {
                    tracing::info!("Tray host gone with no window: showing the window");
                    self.activate();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::profile::ProfileId;

    #[test]
    fn closing_keeps_running_only_in_the_background_and_not_quitting() {
        assert_eq!(close_action(true, false), CloseAction::Background);
        assert_eq!(close_action(false, false), CloseAction::Quit);
        assert_eq!(close_action(true, true), CloseAction::Quit, "Quit");
        assert_eq!(close_action(false, true), CloseAction::Quit);
    }

    #[test]
    fn only_a_lost_host_with_no_window_brings_the_window_back() {
        assert!(recovers_window(WatcherReport::Lost, false, false));
        assert!(!recovers_window(WatcherReport::Lost, true, false), "shown");
        assert!(
            !recovers_window(WatcherReport::Applied, false, false),
            "there never was a host: GNOME's Background Apps leads back"
        );
        assert!(!recovers_window(WatcherReport::Obsolete, false, false));
    }

    #[test]
    fn turning_background_off_with_no_window_shows_one() {
        assert!(shows_window_without_background(false, false, false));
        assert!(
            !shows_window_without_background(false, false, true),
            "shown"
        );
        assert!(
            !shows_window_without_background(true, false, false),
            "turned on"
        );
        assert!(
            !shows_window_without_background(false, true, false),
            "quitting: Quit turns nothing back on"
        );
    }

    /// Dialogs as numbers, top last; a stuck one stays listed after its close.
    struct FakeDialogs {
        shown: RefCell<Vec<u32>>,
        stuck: Option<u32>,
        closed: RefCell<Vec<u32>>,
    }

    impl FakeDialogs {
        fn new(shown: &[u32], stuck: Option<u32>) -> Self {
            Self {
                shown: RefCell::new(shown.to_vec()),
                stuck,
                closed: RefCell::new(Vec::new()),
            }
        }
    }

    impl DialogStack for FakeDialogs {
        type Dialog = u32;

        fn top_dialog(&self) -> Option<u32> {
            self.shown.borrow().last().copied()
        }

        fn force_close(&self, dialog: &u32) {
            self.closed.borrow_mut().push(*dialog);
            if self.stuck != Some(*dialog) {
                self.shown.borrow_mut().pop();
            }
        }
    }

    #[test]
    fn every_dialog_closes_before_the_window() {
        // Preferences over the window, the uinput subpage's dialog over it: Quit and "Hide GX52"
        // used to close only the top one and leave the window shown.
        let dialogs = FakeDialogs::new(&[1, 2], None);
        close_dialogs(&dialogs);
        assert_eq!(*dialogs.closed.borrow(), [2, 1]);
        assert!(dialogs.shown.borrow().is_empty());
        let none = FakeDialogs::new(&[], None);
        close_dialogs(&none);
        assert!(none.closed.borrow().is_empty());
    }

    #[test]
    fn a_dialog_still_shown_after_its_close_ends_the_loop() {
        let dialogs = FakeDialogs::new(&[1, 2], Some(2));
        close_dialogs(&dialogs);
        assert_eq!(
            *dialogs.closed.borrow(),
            [2],
            "closed once, no endless loop"
        );
    }

    #[test]
    fn only_clicks_that_show_the_window_hand_it_their_token() {
        assert!(shows_window(&TrayAction::ToggleWindow, false), "Show");
        assert!(!shows_window(&TrayAction::ToggleWindow, true), "Hide");
        assert!(shows_window(&TrayAction::Preferences, true));
        assert!(shows_window(&TrayAction::About, false));
        let id = ProfileId::parse("abc").expect("a valid id");
        assert!(!shows_window(&TrayAction::ActivateProfile(id), false));
        assert!(!shows_window(&TrayAction::Quit, true));
    }

    #[test]
    fn a_lost_host_while_quitting_leaves_no_window() {
        // Quit with a save pending keeps the process for a moment: the host going away then must
        // not open a window that would keep it running.
        assert!(!recovers_window(WatcherReport::Lost, false, true));
    }
}
