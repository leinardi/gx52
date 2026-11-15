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

//! Start on Login and the hidden start (docs/architecture.md, "Background and autostart"): the
//! "autostart" key asks the Background portal for an autostart entry that starts `gx52 --hidden`,
//! and a hidden start runs in the background with no window.

use std::str::FromStr;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use ashpd::desktop::ResponseError;
use ashpd::desktop::background::Background;
use gettextrs::gettext;
use gtk::{gio, glib};

use super::Application;
use crate::config;
use crate::preferences::{AUTOSTART, RUN_IN_BACKGROUND};
use crate::window::Window;

/// The command-line option of a start that shows no window.
pub(super) const HIDDEN: &str = "hidden";

/// The command line of the autostart entry.
const AUTOSTART_COMMAND: [&str; 2] = ["gx52", "--hidden"];

/// How long a portal request may take: long enough for a permission dialog, short enough that a
/// portal that never answers does not leave Start on Login waiting until the next start.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// How long Quit waits for a portal request: a disable answers at once, a dialog left open would
/// keep GX52 running.
const QUIT_PORTAL_WAIT: Duration = Duration::from_secs(5);

/// What the Background portal answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reply {
    /// The request went through; the autostart entry is now on or off.
    Done { auto_start: bool },
    /// The dialog was dismissed: the entry is as it was.
    Cancelled,
    /// The portal could not be asked, or failed.
    Failed,
}

/// Why an autostart the user still wants was not set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The user, or the permission stored for GX52, refused it.
    Denied,
    /// The portal is missing or failed.
    Unavailable,
}

/// Start on Login as the preferences show it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutostartStatus {
    /// An enable is with the portal.
    pub(crate) pending: bool,
    /// The last enable was refused, and nothing turned Start on Login off since.
    pub(crate) failure: Option<Refusal>,
}

/// Why a request is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// The key changed.
    Change,
    /// A start re-asks for the entry the key already wants.
    Refresh,
}

/// What follows a portal request.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Finished {
    /// The next request to send: the wish changed while this one ran.
    next: Option<bool>,
    /// An autostart that is wanted and was not set up: the key goes back off.
    refusal: Option<Refusal>,
}

/// Keeps the autostart entry in line with the key, one portal request at a time.
///
/// A request has a side effect, so a stale reply cannot simply be dropped: a delayed enable would
/// leave the entry on after a later disable. Requests are serialized instead, and the one that
/// finishes sends the latest wish if it differs, so the last request is always the latest wish.
#[derive(Debug, Default)]
pub(crate) struct AutostartSync {
    /// What the key wants.
    desired: bool,
    /// The request the portal is handling, and why it was sent.
    in_flight: Option<(bool, Cause)>,
    /// The entry as the last reply left it; `None` while unknown.
    entry: Option<bool>,
}

impl AutostartSync {
    /// The key now wants `enabled`, for `cause`: the request to send now, if any.
    fn want(&mut self, enabled: bool, cause: Cause) -> Option<bool> {
        self.desired = enabled;
        if let Some((_, in_flight)) = &mut self.in_flight {
            // The reply now answers a change too.
            if cause == Cause::Change {
                *in_flight = Cause::Change;
            }
            return None;
        }
        if self.entry == Some(enabled) {
            return None;
        }
        self.in_flight = Some((enabled, cause));
        Some(enabled)
    }

    /// The request in flight got `reply`.
    fn finished(&mut self, reply: Reply) -> Finished {
        let Some((sent, cause)) = self.in_flight.take() else {
            return Finished::default();
        };
        self.entry = match reply {
            Reply::Done { auto_start } => Some(auto_start),
            Reply::Cancelled | Reply::Failed => None,
        };
        // A refresh that fails keeps the key: at login the portal may not be ready yet, and the
        // entry is still there. A refusal is the portal's answer either way; xdg-desktop-portal
        // answers a permission turned off with Cancelled, as it does a dismissed dialog.
        let refusal = (sent && self.desired)
            .then_some(match (reply, cause) {
                (Reply::Done { auto_start: true }, _) | (Reply::Failed, Cause::Refresh) => None,
                (Reply::Done { auto_start: false } | Reply::Cancelled, _) => Some(Refusal::Denied),
                (Reply::Failed, Cause::Change) => Some(Refusal::Unavailable),
            })
            .flatten();
        if refusal.is_some() {
            // The key goes back off with nothing more to ask: a dismissed or failed request
            // changed nothing, and an entry left from before ends at the next hidden start.
            self.desired = false;
            self.entry = Some(false);
        }
        let next =
            (self.desired != sent && self.entry != Some(self.desired)).then_some(self.desired);
        self.in_flight = next.map(|enabled| (enabled, Cause::Change));
        Finished { next, refusal }
    }

    /// Whether an enable is with the portal.
    fn enabling(&self) -> bool {
        matches!(self.in_flight, Some((true, _)))
    }
}

/// How a launch starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    /// The window is shown.
    Window,
    /// GX52 runs in the background with no window.
    Background,
    /// Nothing changes: a hidden start while GX52 already runs.
    Nothing,
}

/// What a launch does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaunchPlan {
    start: Start,
    /// A hidden start while Start on Login is off comes from a stale autostart entry, which is
    /// removed.
    removes_stale_entry: bool,
}

/// Which instance handles a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Instance {
    /// The launch started GX52.
    Primary,
    /// GX52 already runs: the launch only forwarded its command line.
    Forwarded,
}

/// A launch from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Launch {
    pub(super) instance: Instance,
    /// With `--hidden`.
    pub(super) hidden: bool,
}

/// The two settings a launch depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackgroundKeys {
    run_in_background: bool,
    autostart: bool,
}

impl Launch {
    /// What this launch does with `keys` as set.
    fn plan(self, keys: BackgroundKeys) -> LaunchPlan {
        let primary_hidden = self.hidden && self.instance == Instance::Primary;
        let start = if !self.hidden {
            Start::Window
        } else if !primary_hidden {
            Start::Nothing
        } else if keys.run_in_background {
            Start::Background
        } else {
            // Nothing would lead back to a window without the tray.
            Start::Window
        };
        LaunchPlan {
            start,
            removes_stale_entry: primary_hidden && !keys.autostart,
        }
    }
}

/// Asks the Background portal for the autostart entry on or off.
async fn request_autostart(enabled: bool) -> Reply {
    let request = glib::future_with_timeout(PORTAL_TIMEOUT, async {
        Background::request()
            .reason(gettext("Apply joystick profiles when you log in").as_str())
            .auto_start(enabled)
            .command(AUTOSTART_COMMAND)
            .dbus_activatable(false)
            .send()
            .await
            .and_then(|request| request.response())
    })
    .await;
    let Ok(request) = request else {
        tracing::warn!("The Background portal did not answer within {PORTAL_TIMEOUT:?}");
        return Reply::Failed;
    };
    match request {
        Ok(response) => Reply::Done {
            auto_start: response.auto_start(),
        },
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => Reply::Cancelled,
        Err(error) => {
            tracing::warn!("The Background portal failed: {error}");
            Reply::Failed
        }
    }
}

/// Registers the host build's app id with the portals, which read it from the sandbox otherwise.
/// Does nothing in the sandbox.
async fn register_host_app() {
    let registered = match ashpd::AppID::from_str(config::APP_ID) {
        Ok(app_id) => ashpd::register_host_app(app_id).await,
        Err(error) => Err(error),
    };
    if let Err(error) = registered {
        // xdg-desktop-portal before 1.19 has no registry: it reads the id from the desktop file
        // of the running process instead.
        tracing::debug!("Cannot register with the portals: {error}");
    }
}

impl Application {
    /// Follows "Start on Login" in `settings`. Called once, before the background rule is
    /// applied, so a key that rule turns off at startup removes the entry too.
    pub(super) fn follow_autostart(&self, settings: &gio::Settings) {
        settings.connect_changed(
            Some(AUTOSTART),
            glib::clone!(
                #[weak(rename_to = app)]
                self,
                move |settings, _| app.want_autostart(settings.boolean(AUTOSTART), Cause::Change)
            ),
        );
        // Run in Background off ends the refusal's warning: Start on Login cannot be on.
        settings.connect_changed(
            Some(RUN_IN_BACKGROUND),
            glib::clone!(
                #[weak(rename_to = app)]
                self,
                move |settings, _| {
                    if !settings.boolean(RUN_IN_BACKGROUND) {
                        app.set_autostart_failure(None);
                    }
                }
            ),
        );
        // Arms `changed`.
        settings.boolean(AUTOSTART);
        settings.boolean(RUN_IN_BACKGROUND);
    }

    /// Start on Login as the preferences show it.
    pub(crate) fn autostart_status(&self) -> AutostartStatus {
        AutostartStatus {
            pending: self.imp().autostart.borrow().enabling(),
            failure: self.imp().autostart_failure.get(),
        }
    }

    /// The refusal the window's banner tells about: one no preferences dialog showed.
    pub(crate) fn autostart_notice(&self) -> Option<Refusal> {
        self.imp()
            .autostart_failure
            .get()
            .filter(|_| self.imp().autostart_notice.get())
    }

    /// `app.autostart-help`, the banner's How to Fix: the preferences on the Start on Login page.
    pub(super) fn show_autostart_help(&self) {
        self.imp().autostart_notice.set(false);
        self.show_preferences_dialog();
        if let Some(dialog) = self.imp().preferences_dialog.upgrade() {
            dialog.show_autostart_help();
        }
        self.show_autostart_status();
    }

    /// Records why Start on Login was refused, or that nothing is refused any more.
    fn set_autostart_failure(&self, failure: Option<Refusal>) {
        let imp = self.imp();
        if failure.is_none() {
            imp.autostart_notice.set(false);
        }
        if imp.autostart_failure.replace(failure) != failure {
            self.show_autostart_status();
        }
    }

    /// Shows Start on Login in the preferences dialog and the windows' banner.
    pub(super) fn show_autostart_status(&self) {
        if let Some(dialog) = self.imp().preferences_dialog.upgrade() {
            dialog.show_autostart(self.autostart_status());
        }
        let notice = self.autostart_notice();
        for window in self.windows() {
            if let Ok(window) = window.downcast::<Window>() {
                window.show_autostart_notice(notice);
            }
        }
    }

    /// Quit: a portal request that does not answer within [`QUIT_PORTAL_WAIT`] no longer keeps
    /// GX52 running.
    pub(super) fn release_portal_on_quit(&self) {
        if self.imp().portal_hold.borrow().is_none() {
            return;
        }
        glib::timeout_add_local_once(
            QUIT_PORTAL_WAIT,
            glib::clone!(
                #[weak(rename_to = app)]
                self,
                move || {
                    if app.imp().quitting.get() && app.imp().portal_hold.take().is_some() {
                        tracing::warn!("Quit before the Background portal answered");
                    }
                }
            ),
        );
    }

    /// Asks for the entry once if "Start on Login" is on, so it follows a moved or updated GX52.
    /// Called once, in `startup`, after the settings.
    pub(super) fn refresh_autostart(&self) {
        if self
            .imp()
            .settings
            .get()
            .is_some_and(|settings| settings.boolean(AUTOSTART))
        {
            self.want_autostart(true, Cause::Refresh);
        }
    }

    /// Starts as the command line asks: the window, or with `--hidden` in the background.
    pub(super) fn launch(&self, launch: Launch) {
        let Some(settings) = self.imp().settings.get() else {
            self.activate();
            return;
        };
        let plan = launch.plan(BackgroundKeys {
            run_in_background: self.runs_in_background(),
            autostart: settings.boolean(AUTOSTART),
        });
        if plan.removes_stale_entry {
            tracing::info!("Started from an autostart entry while Start on Login is off");
            self.want_autostart(false, Cause::Change);
        }
        match plan.start {
            Start::Window => self.activate(),
            Start::Background => {
                tracing::info!("Started in the background");
                self.hold_in_background();
            }
            Start::Nothing => tracing::debug!("Already running: a hidden start changes nothing"),
        }
    }

    fn want_autostart(&self, enabled: bool, cause: Cause) {
        let was_desired = self.imp().autostart.borrow().desired;
        let request = self.imp().autostart.borrow_mut().want(enabled, cause);
        if let Some(enabled) = request {
            self.send_autostart(enabled);
        }
        if was_desired && !enabled {
            // Turned off: nothing is refused any more. A refusal turns the key off after it set
            // the wish off itself, so it does not get here.
            self.set_autostart_failure(None);
        }
        self.show_autostart_status();
    }

    fn send_autostart(&self, enabled: bool) {
        let first = !self.imp().portal_registered.replace(true);
        // A request sent just before Quit still reaches the portal; Quit lets go of it after a
        // while, should the portal never answer. One sent while quitting (a follow-up) holds
        // nothing: that wait is over.
        if !self.imp().quitting.get() {
            self.imp().portal_hold.replace(Some(self.hold()));
        }
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                if first {
                    register_host_app().await;
                }
                let reply = request_autostart(enabled).await;
                tracing::info!("Autostart entry requested {enabled}: {reply:?}");
                app.on_autostart_reply(reply);
            }
        ));
    }

    fn on_autostart_reply(&self, reply: Reply) {
        self.imp().portal_hold.take();
        let finished = self.imp().autostart.borrow_mut().finished(reply);
        if let Some(refusal) = finished.refusal {
            // The preferences show the refusal under the switch; with none showing, the window's
            // banner tells about it when it shows (at login GX52 starts with no window).
            let shown = self
                .imp()
                .preferences_dialog
                .upgrade()
                .is_some_and(|dialog| dialog.is_mapped());
            self.imp().autostart_notice.set(!shown);
            self.imp().autostart_failure.set(Some(refusal));
            if let Some(settings) = self.imp().settings.get()
                && let Err(error) = settings.set_boolean(AUTOSTART, false)
            {
                tracing::warn!("Cannot turn off {AUTOSTART}: {error}");
            }
        } else if reply == (Reply::Done { auto_start: true })
            && self.imp().autostart.borrow().desired
        {
            self.set_autostart_failure(None);
        }
        if let Some(enabled) = finished.next {
            self.send_autostart(enabled);
        }
        self.show_autostart_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: Reply = Reply::Done { auto_start: true };
    const OFF: Reply = Reply::Done { auto_start: false };

    #[test]
    fn a_delayed_enable_is_followed_by_the_disable_that_came_after_it() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        assert_eq!(
            sync.want(false, Cause::Change),
            None,
            "waits for the enable"
        );
        let finished = sync.finished(ON);
        assert_eq!(finished.next, Some(false), "the entry must not stay on");
        assert_eq!(finished.refusal, None);
        assert_eq!(sync.finished(OFF), Finished::default());
        assert_eq!(sync.entry, Some(false));
    }

    #[test]
    fn toggles_while_a_request_runs_end_in_one_follow_up() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        for enabled in [false, true, false] {
            assert_eq!(sync.want(enabled, Cause::Change), None);
        }
        assert_eq!(sync.finished(ON).next, Some(false));
        assert_eq!(
            sync.want(true, Cause::Change),
            None,
            "the follow-up is in flight"
        );
        assert_eq!(sync.finished(OFF).next, Some(true), "the latest wish");
        assert_eq!(sync.finished(ON), Finished::default());
    }

    #[test]
    fn toggling_back_before_the_reply_sends_nothing_more() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        assert_eq!(sync.want(false, Cause::Change), None);
        assert_eq!(sync.want(true, Cause::Change), None);
        assert_eq!(sync.finished(ON), Finished::default());
    }

    #[test]
    fn a_denied_enable_still_wanted_turns_the_key_off() {
        for (reply, refusal) in [
            (OFF, Refusal::Denied),
            (Reply::Cancelled, Refusal::Denied),
            (Reply::Failed, Refusal::Unavailable),
        ] {
            let mut sync = AutostartSync::default();
            assert_eq!(sync.want(true, Cause::Change), Some(true));
            let finished = sync.finished(reply);
            assert_eq!(finished.refusal, Some(refusal), "{reply:?}");
            assert_eq!(finished.next, None, "{reply:?}: nothing to undo");
            assert_eq!(
                sync.want(false, Cause::Change),
                None,
                "{reply:?}: the key turned off asks nothing"
            );
            assert_eq!(
                sync.want(true, Cause::Change),
                Some(true),
                "{reply:?}: asked again when turned on"
            );
        }
    }

    #[test]
    fn a_denial_after_the_user_turned_it_off_does_nothing() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        assert_eq!(sync.want(false, Cause::Change), None);
        assert_eq!(sync.finished(OFF), Finished::default());
    }

    #[test]
    fn a_refresh_that_fails_keeps_the_key() {
        // At login the portal may not be ready: the entry is still there, and losing the key
        // would remove it at the next hidden start.
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Refresh), Some(true));
        assert_eq!(sync.finished(Reply::Failed), Finished::default());
        assert!(sync.desired);
        assert_eq!(sync.want(true, Cause::Change), Some(true), "asked again");
    }

    #[test]
    fn a_refresh_the_portal_refuses_turns_the_key_off() {
        // A permission turned off answers Cancelled.
        for reply in [OFF, Reply::Cancelled] {
            let mut sync = AutostartSync::default();
            assert_eq!(sync.want(true, Cause::Refresh), Some(true));
            assert_eq!(
                sync.finished(reply).refusal,
                Some(Refusal::Denied),
                "{reply:?}"
            );
        }
    }

    #[test]
    fn only_an_enable_is_pending() {
        let mut sync = AutostartSync::default();
        assert!(!sync.enabling());
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        assert!(sync.enabling());
        assert_eq!(sync.finished(ON).next, None);
        assert!(!sync.enabling());
        assert_eq!(sync.want(false, Cause::Change), Some(false));
        assert!(!sync.enabling(), "a disable");
    }

    #[test]
    fn a_change_during_a_refresh_is_a_change() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Refresh), Some(true));
        assert_eq!(sync.want(false, Cause::Change), None);
        assert_eq!(sync.want(true, Cause::Change), None);
        assert_eq!(
            sync.finished(Reply::Failed).refusal,
            Some(Refusal::Unavailable)
        );
    }

    #[test]
    fn a_failed_disable_is_not_retried() {
        let mut sync = AutostartSync::default();
        assert_eq!(
            sync.want(false, Cause::Change),
            Some(false),
            "the entry is unknown"
        );
        assert_eq!(sync.finished(Reply::Failed), Finished::default());
    }

    #[test]
    fn an_entry_known_to_match_needs_no_request() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.want(true, Cause::Change), Some(true));
        assert_eq!(sync.finished(ON), Finished::default());
        assert_eq!(sync.want(true, Cause::Change), None);
        assert_eq!(sync.want(false, Cause::Change), Some(false));
    }

    #[test]
    fn a_reply_with_nothing_in_flight_is_ignored() {
        let mut sync = AutostartSync::default();
        assert_eq!(sync.finished(ON), Finished::default());
        assert_eq!(sync.entry, None);
    }

    #[test]
    fn every_launch_is_planned() {
        let plan = |start, removes_stale_entry| LaunchPlan {
            start,
            removes_stale_entry,
        };
        let launch = |instance, hidden| Launch { instance, hidden };
        let keys = |run_in_background, autostart| BackgroundKeys {
            run_in_background,
            autostart,
        };
        for instance in [Instance::Primary, Instance::Forwarded] {
            for run_in_background in [false, true] {
                for autostart in [false, true] {
                    let keys = keys(run_in_background, autostart);
                    let shown = launch(instance, false).plan(keys);
                    assert_eq!(shown, plan(Start::Window, false), "no --hidden");
                    let forwarded = launch(Instance::Forwarded, true).plan(keys);
                    assert_eq!(forwarded, plan(Start::Nothing, false), "already running");
                }
            }
        }
        let hidden = launch(Instance::Primary, true);
        assert_eq!(
            hidden.plan(keys(true, true)),
            plan(Start::Background, false),
            "the autostart"
        );
        assert_eq!(
            hidden.plan(keys(true, false)),
            plan(Start::Background, true),
            "a stale entry"
        );
        assert_eq!(
            hidden.plan(keys(false, false)),
            plan(Start::Window, true),
            "no tray to lead back to the window"
        );
        assert_eq!(
            hidden.plan(keys(false, true)),
            plan(Start::Window, false),
            "stored inconsistent and not normalized yet: the window, the entry stays"
        );
    }
}
