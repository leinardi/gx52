//! Spike S5: a `ksni` `StatusNotifierItem` tray icon driving a GTK 4 / libadwaita window.
//!
//! Tray callbacks run on ksni's executor thread, so they only send a [`TrayAction`] through an
//! `async_channel`; a `glib::spawn_future_local` loop applies it on the GTK main thread.
//! Throwaway code: findings go to `docs/spikes/05-tray.md`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use ksni::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};

const APP_ID: &str = "com.leinardi.gx52.TraySpike";

/// What the tray asks the main thread to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    ToggleWindow,
    Quit,
    WatcherOnline,
    WatcherOffline,
}

/// The tray item; lives on ksni's executor thread, so it holds only a `Send` channel end.
#[derive(Debug)]
struct Gx52Tray {
    actions: async_channel::Sender<TrayAction>,
}

impl Gx52Tray {
    fn send(&self, action: TrayAction) {
        // The receiver only goes away when the app quits; nothing left to notify then.
        if let Err(err) = self.actions.try_send(action) {
            tracing::debug!("dropping tray action {action:?}: {err}");
        }
    }
}

impl ksni::Tray for Gx52Tray {
    fn id(&self) -> String {
        APP_ID.into()
    }

    fn title(&self) -> String {
        "GX52".into()
    }

    fn icon_name(&self) -> String {
        "input-gaming-symbolic".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Show / Hide".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::ToggleWindow)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_online(&self) {
        self.send(TrayAction::WatcherOnline);
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        tracing::warn!("tray watcher offline: {reason:?}");
        self.send(TrayAction::WatcherOffline);
        // Keep the service alive so the icon comes back if the watcher returns.
        true
    }
}

/// Main-thread state shared by the window and the tray loop.
#[derive(Debug, Default)]
struct TrayState {
    /// Whether a `StatusNotifierWatcher` is showing our icon; `None` until the first report.
    ///
    /// With `assume_sni_available(true)`, `spawn()` succeeds even without a watcher and the
    /// failure arrives through `watcher_offline`, possibly before `spawn()` returns, so a
    /// successful spawn must not overwrite an earlier offline report.
    available: Cell<Option<bool>>,
    /// Keeps the application alive while the window is hidden.
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
}

fn build_window(app: &adw::Application, state: &Rc<TrayState>) -> adw::ApplicationWindow {
    let page = adw::StatusPage::builder()
        .icon_name("input-gaming-symbolic")
        .title("GX52 tray spike")
        .description("Close the window: it hides when a tray is available, quits otherwise.")
        .build();
    let content = adw::ToolbarView::new();
    content.add_top_bar(&adw::HeaderBar::new());
    content.set_content(Some(&page));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("GX52 tray spike")
        .default_width(480)
        .default_height(360)
        .content(&content)
        .build();

    window.connect_close_request(glib::clone!(
        #[weak]
        app,
        #[strong]
        state,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |window| {
            if state.available.get() == Some(true) {
                tracing::info!("close request: hiding to tray");
                state.hold.replace(Some(app.hold()));
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                tracing::info!("close request: no tray, quitting");
                glib::Propagation::Proceed
            }
        }
    ));
    window
}

fn toggle_window(app: &adw::Application, state: &TrayState) {
    let Some(window) = app.windows().into_iter().next() else {
        return;
    };
    if window.is_visible() {
        state.hold.replace(Some(app.hold()));
        window.set_visible(false);
        tracing::info!("tray action applied: window hidden");
    } else {
        show_window(app, state);
        tracing::info!("tray action applied: window shown");
    }
}

/// Shows the window and drops the hold taken while it was hidden in the tray.
fn show_window(app: &adw::Application, state: &TrayState) {
    state.hold.replace(None);
    if let Some(window) = app.windows().into_iter().next() {
        window.present();
    }
}

fn spawn_tray(app: &adw::Application, state: &Rc<TrayState>) {
    let (sender, receiver) = async_channel::bounded(16);
    let sandboxed = std::path::Path::new("/.flatpak-info").exists();

    glib::spawn_future_local(glib::clone!(
        #[strong]
        state,
        async move {
            let tray = Gx52Tray { actions: sender };
            match tray
                .disable_dbus_name(sandboxed)
                .assume_sni_available(true)
                .spawn()
                .await
            {
                Ok(_handle) => {
                    tracing::info!(
                        "tray spawned (sandboxed: {sandboxed}), available: {:?}",
                        state.available.get()
                    );
                    // watcher_online is not called for the initial registration.
                    if state.available.get().is_none() {
                        state.available.set(Some(true));
                    }
                    // Keep the handle alive for the lifetime of the app.
                    std::future::pending::<()>().await;
                }
                Err(err) => tracing::error!("tray could not be spawned: {err}"),
            }
        }
    ));

    glib::spawn_future_local(glib::clone!(
        #[weak]
        app,
        #[strong]
        state,
        async move {
            while let Ok(action) = receiver.recv().await {
                tracing::info!("tray action received on main thread: {action:?}");
                tracing::debug!("tray availability before: {:?}", state.available.get());
                match action {
                    TrayAction::ToggleWindow => toggle_window(&app, &state),
                    TrayAction::Quit => app.quit(),
                    TrayAction::WatcherOnline => state.available.set(Some(true)),
                    TrayAction::WatcherOffline => {
                        state.available.set(Some(false));
                        // Without a tray there is no way back to a hidden window.
                        show_window(&app, &state);
                    }
                }
            }
        }
    ));
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "s5_tray=info".into()),
        )
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();
    let state = Rc::new(TrayState::default());
    app.connect_startup(glib::clone!(
        #[strong]
        state,
        move |app| spawn_tray(app, &state)
    ));
    app.connect_activate(move |app| {
        if app.windows().is_empty() {
            build_window(app, &state);
        }
        show_window(app, &state);
    });
    app.run()
}
