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

//! The Start on Login guidance (docs/ui.md, "Guidance pages"): a subpage of the preferences
//! dialog that says why the desktop refused Start on Login, how to fix it, and asks again.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::application::{AutostartStatus, Refusal};

/// Where a Try Again stands.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Retry {
    /// None asked, or the last one is answered.
    #[default]
    Idle,
    /// Asked: the key turned on, the request is not sent yet.
    Asked,
    /// The request is with the portal.
    Pending,
}

impl Retry {
    /// The next state for `status`, and whether the retry just got its answer.
    fn follow(self, status: AutostartStatus) -> (Self, bool) {
        match (self, status.pending) {
            (Self::Asked | Self::Pending, true) => (Self::Pending, false),
            (Self::Pending, false) => (Self::Idle, true),
            (Self::Asked | Self::Idle, false) | (Self::Idle, true) => (self, false),
        }
    }
}

mod imp {
    use std::cell::Cell;

    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gtk::glib;

    use crate::application::Refusal;
    use crate::config;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/autostart-page.ui")]
    pub struct AutostartPage {
        #[template_child]
        pub(super) page: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) gnome_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) other_desktops_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) terminal_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) portal_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) command_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) copy_command: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) retry_failed: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) try_again: TemplateChild<adw::ButtonRow>,
        /// The reason shown.
        pub(super) reason: Cell<Option<Refusal>>,
        pub(super) retry: Cell<super::Retry>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AutostartPage {
        const NAME: &'static str = "GX52AutostartPage";
        type Type = super::AutostartPage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AutostartPage {
        fn constructed(&self) {
            self.parent_constructed();
            let command = super::permission_command(config::APP_ID);
            self.command_row.set_title(&command);
            self.copy_command
                .connect_clicked(move |button| super::super::copy_in_dialog(button, &command));
            let page = self.obj();
            self.try_again.connect_activated(glib::clone!(
                #[weak]
                page,
                move |_| page.try_again()
            ));
        }
    }

    impl WidgetImpl for AutostartPage {}
    impl NavigationPageImpl for AutostartPage {}
}

glib::wrapper! {
    pub struct AutostartPage(ObjectSubclass<imp::AutostartPage>)
        @extends adw::NavigationPage, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AutostartPage {
    /// A new page for `status`, to push onto the preferences dialog.
    pub(crate) fn new(status: AutostartStatus) -> Self {
        let page: Self = glib::Object::new();
        page.show(status);
        page
    }

    /// Follows `status`: the reason, a Try Again on its way, and its answer. A Try Again that
    /// worked closes the page.
    pub(crate) fn show(&self, status: AutostartStatus) {
        let imp = self.imp();
        if let Some(reason) = status.failure {
            self.show_reason(reason);
        }
        let (retry, answered) = imp.retry.get().follow(status);
        imp.retry.set(retry);
        imp.try_again.set_sensitive(!status.pending);
        imp.try_again.set_title(&if status.pending {
            gettext("Asking the Desktop…")
        } else {
            gettext("_Try Again")
        });
        if status.pending {
            imp.retry_failed.set_visible(false);
        }
        if !answered {
            return;
        }
        match status.failure {
            None => {
                if let Some(dialog) = self
                    .ancestor(adw::PreferencesDialog::static_type())
                    .and_downcast::<adw::PreferencesDialog>()
                {
                    dialog.pop_subpage();
                }
            }
            Some(reason) => {
                imp.retry_failed.set_title(&match reason {
                    Refusal::Denied => gettext("Still not allowed by the desktop"),
                    Refusal::Unavailable => gettext("The Background portal is still not available"),
                });
                imp.retry_failed.set_visible(true);
            }
        }
    }

    fn show_reason(&self, reason: Refusal) {
        let imp = self.imp();
        if imp.reason.replace(Some(reason)) == Some(reason) {
            return;
        }
        let denied = reason == Refusal::Denied;
        for group in [
            &*imp.gnome_group,
            &*imp.other_desktops_group,
            &*imp.terminal_group,
        ] {
            group.set_visible(denied);
        }
        imp.portal_group.set_visible(!denied);
        imp.page.set_description(&match reason {
            Refusal::Denied => gettext(
                "The desktop doesn’t allow GX52 to run in the background, so GX52 won’t start when you log in. The same permission keeps GX52 running with its window closed: without it, the desktop closes GX52 a few seconds after its last window closes, even with Run in Background on, and your profiles stop being applied.",
            ),
            Refusal::Unavailable => gettext(
                "The desktop has no working Background portal, so GX52 can’t ask to be started when you log in.",
            ),
        });
    }

    /// Turns Start on Login on again; the answer comes through [`Self::show`].
    fn try_again(&self) {
        let imp = self.imp();
        imp.retry.set(Retry::Asked);
        imp.retry_failed.set_visible(false);
        if let Err(error) = self.activate_action("preferences.autostart-retry", None) {
            tracing::warn!("Cannot ask for Start on Login again: {error}");
            imp.retry.set(Retry::Idle);
        }
    }
}

/// The command that gives `app_id` the background permission back.
fn permission_command(app_id: &str) -> String {
    format!("flatpak permission-set background background {app_id} yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PENDING: AutostartStatus = AutostartStatus {
        pending: true,
        failure: None,
    };
    const DONE: AutostartStatus = AutostartStatus {
        pending: false,
        failure: None,
    };

    #[test]
    fn a_retry_is_answered_once_its_request_finishes() {
        let (retry, answered) = Retry::Asked.follow(PENDING);
        assert_eq!((retry, answered), (Retry::Pending, false));
        assert_eq!(retry.follow(DONE), (Retry::Idle, true));
    }

    #[test]
    fn a_status_before_the_request_is_sent_answers_nothing() {
        assert_eq!(Retry::Asked.follow(DONE), (Retry::Asked, false));
    }

    #[test]
    fn a_request_nobody_retried_answers_nothing() {
        assert_eq!(Retry::Idle.follow(PENDING), (Retry::Idle, false));
        assert_eq!(Retry::Idle.follow(DONE), (Retry::Idle, false));
    }

    #[test]
    fn the_command_names_the_build() {
        assert_eq!(
            permission_command("com.leinardi.gx52.Devel"),
            "flatpak permission-set background background com.leinardi.gx52.Devel yes"
        );
    }
}
