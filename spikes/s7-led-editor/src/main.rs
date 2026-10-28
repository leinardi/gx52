//! Spike S7: visual LED editor for the X52 Pro with placeholder artwork.
//!
//! Each view shows one side of the device (`gtk::Svg` artwork in a [`HotspotView`]) with a
//! focusable button per LED; the button opens a popover to pick the LED state. Throwaway code:
//! findings go to `docs/spikes/07-led-editor.md`.

mod hotspot_view;

use adw::prelude::*;
use gtk::{gdk, glib};

use crate::hotspot_view::HotspotView;

const APP_ID: &str = "com.leinardi.gx52.LedEditorSpike";

/// The LED kinds of the X52 Pro: single-colour LEDs are on/off, the others off/green/red/amber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LedKind {
    Single,
    BiColour,
}

/// Selectable states, as `(name, label)`; the name is also the CSS class suffix.
const SINGLE_STATES: &[(&str, &str)] = &[("off", "Off"), ("on", "On")];
const BI_COLOUR_STATES: &[(&str, &str)] = &[
    ("off", "Off"),
    ("green", "Green"),
    ("red", "Red"),
    ("amber", "Amber"),
];

impl LedKind {
    fn states(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Single => SINGLE_STATES,
            Self::BiColour => BI_COLOUR_STATES,
        }
    }
}

/// One LED on one view: name shown to the user and its position on that view's artwork.
#[derive(Debug, Clone, Copy)]
struct LedSpot {
    label: &'static str,
    kind: LedKind,
    x: f32,
    y: f32,
}

/// One side of the device.
#[derive(Debug)]
struct DeviceView {
    name: &'static str,
    title: &'static str,
    icon: &'static str,
    svg: &'static str,
    leds: &'static [LedSpot],
}

const fn led(label: &'static str, kind: LedKind, x: f32, y: f32) -> LedSpot {
    LedSpot { label, kind, x, y }
}

/// Placeholder artwork: rough outlines only, to be replaced by traced drawings of the device.
const VIEWS: &[DeviceView] = &[
    DeviceView {
        name: "stick",
        title: "Stick",
        icon: "input-gaming-symbolic",
        svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300" viewBox="0 0 400 300">
  <path d="M150 290 L140 120 Q140 40 200 30 Q260 40 260 120 L250 290 Z" fill="none" stroke="#8a8a8a" stroke-width="4"/>
  <rect x="165" y="60" width="70" height="40" rx="8" fill="none" stroke="#8a8a8a" stroke-width="3"/>
  <text x="200" y="280" font-size="14" text-anchor="middle" fill="#8a8a8a">stick grip (placeholder)</text>
</svg>"##,
        leds: &[
            led("Fire", LedKind::Single, 0.50, 0.27),
            led("A", LedKind::BiColour, 0.38, 0.15),
            led("B", LedKind::BiColour, 0.62, 0.15),
            led("POV 2", LedKind::BiColour, 0.50, 0.45),
        ],
    },
    DeviceView {
        name: "base",
        title: "Stick base",
        icon: "view-grid-symbolic",
        svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200" viewBox="0 0 400 200">
  <rect x="20" y="40" width="360" height="120" rx="20" fill="none" stroke="#8a8a8a" stroke-width="4"/>
  <text x="200" y="185" font-size="14" text-anchor="middle" fill="#8a8a8a">stick base, toggles (placeholder)</text>
</svg>"##,
        leds: &[
            led("T1/T2", LedKind::BiColour, 0.25, 0.50),
            led("T3/T4", LedKind::BiColour, 0.50, 0.50),
            led("T5/T6", LedKind::BiColour, 0.75, 0.50),
        ],
    },
    DeviceView {
        name: "throttle",
        title: "Throttle",
        icon: "media-playlist-shuffle-symbolic",
        svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300" viewBox="0 0 400 300">
  <path d="M60 260 L60 110 Q60 50 140 40 L300 40 Q340 50 340 110 L340 260 Z" fill="none" stroke="#8a8a8a" stroke-width="4"/>
  <rect x="250" y="190" width="70" height="50" rx="6" fill="none" stroke="#8a8a8a" stroke-width="3"/>
  <text x="200" y="285" font-size="14" text-anchor="middle" fill="#8a8a8a">throttle (placeholder)</text>
</svg>"##,
        leds: &[
            led("D", LedKind::BiColour, 0.30, 0.30),
            led("E", LedKind::BiColour, 0.45, 0.30),
            led("i", LedKind::BiColour, 0.62, 0.42),
            led("Throttle", LedKind::Single, 0.30, 0.70),
        ],
    },
];

// `GtkMenuButton` is a `menubutton` node wrapping the real `button`: classes go on the former,
// the look on the latter.
const CSS: &str = "
menubutton.led > button {
  min-width: 28px;
  min-height: 28px;
  padding: 0;
  border-radius: 9999px;
  box-shadow: 0 0 0 2px var(--window-bg-color), 0 0 0 3px alpha(currentColor, 0.3);
}
menubutton.led.led-off > button { background-color: alpha(currentColor, 0.12); }
menubutton.led.led-on > button, menubutton.led.led-green > button { background-color: var(--green-3); }
menubutton.led.led-red > button { background-color: var(--red-3); }
menubutton.led.led-amber > button { background-color: var(--yellow-3); }
";

/// Updates the hotspot's look and accessible description for a new state.
fn apply_led_state(button: &gtk::MenuButton, spot: &LedSpot, state: &str) {
    for (name, _) in spot.kind.states() {
        button.remove_css_class(&format!("led-{name}"));
    }
    button.add_css_class(&format!("led-{state}"));
    let label = spot
        .kind
        .states()
        .iter()
        .find(|(name, _)| *name == state)
        .map_or(state, |(_, label)| label);
    let description = format!("{} LED: {label}", spot.label);
    button.set_tooltip_text(Some(&description));
    button.update_property(&[gtk::accessible::Property::Label(&description)]);
}

/// A round button for one LED; it opens a popover with the LED's possible states.
fn led_button(spot: LedSpot) -> gtk::MenuButton {
    let group = adw::ToggleGroup::new();
    for (name, label) in spot.kind.states() {
        group.add(adw::Toggle::builder().name(*name).label(*label).build());
    }
    let initial = match spot.kind {
        LedKind::Single => "on",
        LedKind::BiColour => "green",
    };
    group.set_active_name(Some(initial));

    let title = gtk::Label::builder()
        .label(spot.label)
        .css_classes(["heading"])
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.append(&title);
    content.append(&group);
    let popover = gtk::Popover::builder().child(&content).build();

    let button = gtk::MenuButton::builder()
        .popover(&popover)
        // An empty child: without one the button shows a drop-down arrow.
        .child(&gtk::Box::new(gtk::Orientation::Horizontal, 0))
        .css_classes(["led"])
        .build();
    apply_led_state(&button, &spot, initial);

    group.connect_active_name_notify(glib::clone!(
        #[weak]
        button,
        move |group| {
            if let Some(state) = group.active_name() {
                tracing::info!("{} -> {state}", spot.label);
                apply_led_state(&button, &spot, &state);
            }
        }
    ));
    button
}

fn device_page(view: &DeviceView) -> HotspotView {
    let hotspots = HotspotView::default();
    hotspots.set_margin_top(12);
    hotspots.set_margin_bottom(12);
    hotspots.set_margin_start(12);
    hotspots.set_margin_end(12);
    hotspots.set_paintable(Some(&gtk::Svg::from_bytes(&glib::Bytes::from_static(
        view.svg.as_bytes(),
    ))));
    hotspots.update_property(&[gtk::accessible::Property::Label(view.title)]);
    for spot in view.leds {
        hotspots.add_hotspot(&led_button(*spot), spot.x, spot.y);
    }
    hotspots
}

fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let stack = adw::ViewStack::new();
    for view in VIEWS {
        stack.add_titled_with_icon(&device_page(view), Some(view.name), view.title, view.icon);
    }
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));

    adw::ApplicationWindow::builder()
        .application(app)
        .title("GX52 LED editor spike")
        .default_width(720)
        .default_height(560)
        .width_request(360)
        .height_request(294)
        .content(&toolbar)
        .build()
}

/// Test aid: renders every view of `window` to `<dir>/<view>.png`, then quits the app.
fn capture_views(app: &adw::Application, window: &adw::ApplicationWindow, dir: std::path::PathBuf) {
    let Some(stack) = window
        .content()
        .and_downcast::<adw::ToolbarView>()
        .and_then(|toolbar| toolbar.content())
        .and_downcast::<adw::ViewStack>()
    else {
        return;
    };
    let names: Vec<&'static str> = VIEWS.iter().map(|view| view.name).collect();
    let index = std::cell::Cell::new(0_usize);
    glib::timeout_add_local(
        std::time::Duration::from_millis(700),
        glib::clone!(
            #[weak]
            app,
            #[weak]
            window,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || {
                let current = index.get();
                if current > 0 {
                    save_window_png(&window, &dir.join(format!("{}.png", names[current - 1])));
                }
                if let Some(name) = names.get(current) {
                    stack.set_visible_child_name(name);
                    index.set(current + 1);
                    glib::ControlFlow::Continue
                } else {
                    app.quit();
                    glib::ControlFlow::Break
                }
            }
        ),
    );
}

fn save_window_png(window: &adw::ApplicationWindow, path: &std::path::Path) {
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) else {
        tracing::warn!("nothing to capture for {}", path.display());
        return;
    };
    let texture = renderer.render_texture(node, None);
    match texture.save_to_png(path) {
        Ok(()) => tracing::info!("saved {}", path.display()),
        Err(err) => tracing::warn!("could not save {}: {err}", path.display()),
    }
}

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "s7_led_editor=info".into()),
        )
        .init();

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let window = build_window(app);
        window.present();
        if let Some(dir) = std::env::var_os("GX52_SPIKE_SCREENSHOT_DIR") {
            capture_views(app, &window, dir.into());
        }
    });
    app.run()
}
