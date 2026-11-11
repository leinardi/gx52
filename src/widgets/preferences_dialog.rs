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

//! The preferences dialog (docs/ui.md, "Preferences"): every row bound to its `GSettings`
//! key, and the uinput warning while the virtual mouse cannot work.

use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use super::UinputPage;
use crate::device::access::{self, UinputAccess};
use crate::preferences::{
    self, AUTOSTART, RUN_IN_BACKGROUND, VIRTUAL_MOUSE, VIRTUAL_MOUSE_CURVE, VIRTUAL_MOUSE_DEADZONE,
    VIRTUAL_MOUSE_INVERT_WHEEL, VIRTUAL_MOUSE_SPEED,
};

/// How often the uinput access is checked again while the warning shows, so it goes away by
/// itself once the rule is loaded, as the udev page does.
const UINPUT_RECHECK: Duration = Duration::from_secs(1);

/// The uinput checks of the dialog: at most one runs, and only the result of the latest request
/// is shown. A request or an invalidation while a check runs makes that check stale; its result
/// is then dropped and a fresh check starts.
#[derive(Debug, Default)]
struct UinputChecks {
    /// Bumped by every request and every invalidation.
    generation: u64,
    /// Whether a check runs.
    running: bool,
}

/// A started check, to hand back to [`UinputChecks::finish`] with its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UinputTicket(u64);

/// What to do with a finished check's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UinputFinish {
    /// Show it: nothing changed since the check started.
    Show,
    /// Drop it and request a check again if one is still wanted.
    Stale,
}

impl UinputChecks {
    /// Asks for a check: the ticket of a new one, or `None` while another runs (it turns stale,
    /// and its finish asks for the fresh check).
    fn request(&mut self) -> Option<UinputTicket> {
        self.generation += 1;
        if self.running {
            return None;
        }
        self.running = true;
        Some(UinputTicket(self.generation))
    }

    /// Makes the running check, if any, stale: the virtual mouse turned off or the dialog went.
    fn invalidate(&mut self) {
        self.generation += 1;
    }

    /// Ends the check of `ticket`.
    fn finish(&mut self, ticket: UinputTicket) -> UinputFinish {
        self.running = false;
        if ticket.0 == self.generation {
            UinputFinish::Show
        } else {
            UinputFinish::Stale
        }
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gtk::glib;

    use crate::device::access::UinputAccess;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub(super) run_in_background: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) autostart: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) virtual_mouse: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) speed: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) curve: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) deadzone: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) invert_wheel: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) uinput_warning: TemplateChild<adw::ActionRow>,
        /// The next uinput check, while the warning shows.
        pub(super) uinput_recheck: RefCell<Option<glib::SourceId>>,
        /// The uinput checks, running and requested.
        pub(super) uinput_checks: RefCell<super::UinputChecks>,
        /// The uinput access last found, to log it once per change.
        pub(super) uinput_access: Cell<Option<UinputAccess>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "GX52PreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.install_action("preferences.uinput-help", None, |dialog, _, _| {
                dialog.push_subpage(&super::UinputPage::new());
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let dialog = self.obj();
            dialog.connect_map(super::PreferencesDialog::check_uinput);
            dialog.connect_unmap(super::PreferencesDialog::check_uinput);
            self.virtual_mouse.connect_active_notify(glib::clone!(
                #[weak]
                dialog,
                move |_| dialog.check_uinput()
            ));
        }

        fn dispose(&self) {
            self.obj().stop_uinput_recheck();
        }
    }

    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl PreferencesDialog {
    /// Whether the uinput warning applies: the dialog shows and the virtual mouse is on.
    fn wants_uinput(&self) -> bool {
        self.is_mapped() && self.imp().virtual_mouse.is_active()
    }

    /// Checks the uinput access off the main thread and shows the warning when it is missing;
    /// hides it while the virtual mouse is off.
    fn check_uinput(&self) {
        let imp = self.imp();
        self.stop_uinput_recheck();
        if !self.wants_uinput() {
            imp.uinput_checks.borrow_mut().invalidate();
            imp.uinput_warning.set_visible(false);
            return;
        }
        let Some(ticket) = imp.uinput_checks.borrow_mut().request() else {
            // The running check is stale now; its finish starts a fresh one.
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let result = gio::spawn_blocking(access::check_uinput).await;
                dialog.finish_uinput(ticket, result.ok());
            }
        ));
    }

    /// Ends the check of `ticket`: shows its result, or checks again when the result is stale.
    fn finish_uinput(&self, ticket: UinputTicket, access: Option<UinputAccess>) {
        let finish = self.imp().uinput_checks.borrow_mut().finish(ticket);
        match finish {
            UinputFinish::Show => self.show_uinput(access),
            UinputFinish::Stale => self.check_uinput(),
        }
    }

    /// Shows the result of a current uinput check (`None` when the check itself panicked), then
    /// checks again later while the access is missing.
    fn show_uinput(&self, access: Option<UinputAccess>) {
        let imp = self.imp();
        if let Some(access) = access
            && imp.uinput_access.replace(Some(access)) != Some(access)
        {
            tracing::info!("uinput access: {access:?}");
        }
        let available = access == Some(UinputAccess::Available);
        imp.uinput_warning.set_visible(!available);
        if !available {
            self.stop_uinput_recheck();
            let source = glib::timeout_add_local_once(
                UINPUT_RECHECK,
                glib::clone!(
                    #[weak(rename_to = dialog)]
                    self,
                    move || {
                        // This source is done; only its own id is cleared.
                        dialog.imp().uinput_recheck.take();
                        dialog.check_uinput();
                    }
                ),
            );
            imp.uinput_recheck.replace(Some(source));
        }
    }

    /// Cancels the next uinput check.
    fn stop_uinput_recheck(&self) {
        if let Some(source) = self.imp().uinput_recheck.take() {
            source.remove();
        }
    }

    /// A dialog showing and changing `settings`.
    pub(crate) fn new(settings: &gio::Settings) -> Self {
        let dialog: Self = glib::Object::new();
        dialog.bind(settings);
        dialog
    }

    /// Binds each row to its key; the bindings go with the rows.
    fn bind(&self, settings: &gio::Settings) {
        let imp = self.imp();
        for (key, row) in [
            (RUN_IN_BACKGROUND, &*imp.run_in_background),
            (AUTOSTART, &*imp.autostart),
            (VIRTUAL_MOUSE, &*imp.virtual_mouse),
            (VIRTUAL_MOUSE_INVERT_WHEEL, &*imp.invert_wheel),
        ] {
            settings.bind(key, row, "active").build();
        }
        // Starting hidden needs running in the background (preferences::follow_background turns
        // it off with it).
        settings
            .bind(RUN_IN_BACKGROUND, &*imp.autostart, "sensitive")
            .get_only()
            .build();

        let schema = settings.settings_schema();
        for (key, row) in [
            (VIRTUAL_MOUSE_SPEED, &*imp.speed),
            (VIRTUAL_MOUSE_CURVE, &*imp.curve),
            (VIRTUAL_MOUSE_DEADZONE, &*imp.deadzone),
        ] {
            // The bounds first: binding to an adjustment without them would clamp the value
            // and write it back to the key.
            let Some((lower, upper)) = schema
                .as_ref()
                .and_then(|schema| preferences::spin_bounds(&schema.key(key)))
            else {
                // The schema is compiled with the app; preferences::tests checks every range.
                tracing::warn!("The settings key {key} has no range; its row stays off");
                row.set_sensitive(false);
                continue;
            };
            let adjustment = row.adjustment();
            adjustment.set_lower(lower);
            adjustment.set_upper(upper);
            settings.bind(key, row, "value").build();
            settings
                .bind(VIRTUAL_MOUSE, row, "sensitive")
                .get_only()
                .build();
        }
        settings
            .bind(VIRTUAL_MOUSE, &*imp.invert_wheel, "sensitive")
            .get_only()
            .build();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_result_with_no_change_meanwhile_is_shown() {
        let mut checks = UinputChecks::default();
        let ticket = checks.request().expect("nothing runs");
        assert_eq!(checks.finish(ticket), UinputFinish::Show);
    }

    #[test]
    fn a_delayed_result_after_off_and_on_is_dropped_for_a_fresh_check() {
        let mut checks = UinputChecks::default();
        // The check starts while the access is there; its result is delayed.
        let delayed = checks.request().expect("nothing runs");
        // The switch turns off, the access goes, the switch turns on.
        checks.invalidate();
        assert_eq!(checks.request(), None, "one check at a time");

        assert_eq!(checks.finish(delayed), UinputFinish::Stale);
        let fresh = checks.request().expect("the stale check is done");
        assert_eq!(checks.finish(fresh), UinputFinish::Show);
    }

    #[test]
    fn a_result_after_the_switch_turned_off_is_dropped() {
        let mut checks = UinputChecks::default();
        let ticket = checks.request().expect("nothing runs");
        checks.invalidate();
        assert_eq!(checks.finish(ticket), UinputFinish::Stale);
    }

    #[test]
    fn a_request_while_a_check_runs_makes_it_stale() {
        let mut checks = UinputChecks::default();
        let first = checks.request().expect("nothing runs");
        assert_eq!(checks.request(), None);
        assert_eq!(checks.finish(first), UinputFinish::Stale);
    }
}
