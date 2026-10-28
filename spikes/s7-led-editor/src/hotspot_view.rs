//! `HotspotView`: draws a paintable (the device artwork) scaled to fit, keeping its aspect ratio,
//! and places child widgets at fractional positions on top of it.

use std::cell::RefCell;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};

mod imp {
    use super::{RefCell, gdk, glib, graphene};
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;

    /// A child placed on the artwork, centred on `(x, y)` in `0.0..=1.0` artwork coordinates.
    #[derive(Debug)]
    pub(super) struct Hotspot {
        pub(super) widget: gtk::Widget,
        pub(super) x: f32,
        pub(super) y: f32,
    }

    #[derive(Debug, Default)]
    pub struct HotspotView {
        pub(super) paintable: RefCell<Option<gdk::Paintable>>,
        pub(super) invalidate_handler: RefCell<Option<glib::SignalHandlerId>>,
        pub(super) hotspots: RefCell<Vec<Hotspot>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HotspotView {
        const NAME: &'static str = "Gx52HotspotView";
        type Type = super::HotspotView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("hotspot-view");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for HotspotView {
        fn dispose(&self) {
            self.obj().set_paintable(None::<&gdk::Paintable>);
            for hotspot in self.hotspots.take() {
                hotspot.widget.unparent();
            }
        }
    }

    impl WidgetImpl for HotspotView {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let (width, height) = self.intrinsic_size();
            let natural = match orientation {
                gtk::Orientation::Horizontal => width,
                _ if for_size > 0 => scale_to(for_size, height, width),
                _ => height,
            };
            // Small minimum: the artwork scales down, the hotspots keep their own size.
            (MIN_SIZE.min(natural), natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let area = self.artwork_area(width, height);
            for hotspot in self.hotspots.borrow().iter() {
                let (_, natural) = hotspot.widget.preferred_size();
                let (w, h) = (natural.width(), natural.height());
                #[expect(clippy::cast_possible_truncation, reason = "pixel coordinates")]
                let x = (area.x() + hotspot.x * area.width()).round() as i32 - w / 2;
                #[expect(clippy::cast_possible_truncation, reason = "pixel coordinates")]
                let y = (area.y() + hotspot.y * area.height()).round() as i32 - h / 2;
                hotspot
                    .widget
                    .size_allocate(&gtk::Allocation::new(x, y, w, h), -1);
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            if let Some(paintable) = self.paintable.borrow().as_ref() {
                let area = self.artwork_area(widget.width(), widget.height());
                snapshot.save();
                snapshot.translate(&graphene::Point::new(area.x(), area.y()));
                paintable.snapshot(snapshot, f64::from(area.width()), f64::from(area.height()));
                snapshot.restore();
            }
            for hotspot in self.hotspots.borrow().iter() {
                widget.snapshot_child(&hotspot.widget, snapshot);
            }
        }
    }

    /// Smallest size the view asks for; below it the artwork would be unreadable.
    const MIN_SIZE: i32 = 120;
    /// Size used when no artwork is set yet.
    const FALLBACK_SIZE: (i32, i32) = (400, 300);

    fn scale_to(size: i32, numerator: i32, denominator: i32) -> i32 {
        if denominator == 0 {
            return size;
        }
        let scaled = i64::from(size) * i64::from(numerator) / i64::from(denominator);
        i32::try_from(scaled).unwrap_or(i32::MAX)
    }

    impl HotspotView {
        fn intrinsic_size(&self) -> (i32, i32) {
            self.paintable
                .borrow()
                .as_ref()
                .map(|p| (p.intrinsic_width(), p.intrinsic_height()))
                .filter(|&(w, h)| w > 0 && h > 0)
                .unwrap_or(FALLBACK_SIZE)
        }

        /// The rectangle the artwork occupies inside a `width` × `height` widget.
        #[expect(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^23")]
        fn artwork_area(&self, width: i32, height: i32) -> graphene::Rect {
            let (iw, ih) = self.intrinsic_size();
            let scale = (width as f32 / iw as f32).min(height as f32 / ih as f32);
            let (w, h) = (iw as f32 * scale, ih as f32 * scale);
            graphene::Rect::new((width as f32 - w) / 2.0, (height as f32 - h) / 2.0, w, h)
        }
    }
}

glib::wrapper! {
    /// Device artwork with interactive hotspots placed on it.
    pub struct HotspotView(ObjectSubclass<imp::HotspotView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for HotspotView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl HotspotView {
    /// Sets the artwork; a `None` paintable leaves an empty area of the fallback size.
    pub fn set_paintable(&self, paintable: Option<&impl IsA<gdk::Paintable>>) {
        let imp = self.imp();
        if let Some(old) = imp.paintable.take()
            && let Some(id) = imp.invalidate_handler.take()
        {
            old.disconnect(id);
        }
        if let Some(paintable) = paintable {
            let paintable = paintable.upcast_ref::<gdk::Paintable>();
            let id = paintable.connect_invalidate_contents(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move |_| view.queue_draw()
            ));
            imp.invalidate_handler.replace(Some(id));
            imp.paintable.replace(Some(paintable.clone()));
        }
        self.queue_resize();
    }

    /// Places `widget` centred on `(x, y)`, both in `0.0..=1.0` of the artwork's width and height.
    pub fn add_hotspot(&self, widget: &impl IsA<gtk::Widget>, x: f32, y: f32) {
        let widget = widget.upcast_ref::<gtk::Widget>();
        widget.set_parent(self);
        self.imp().hotspots.borrow_mut().push(imp::Hotspot {
            widget: widget.clone(),
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
        });
        self.queue_resize();
    }
}
