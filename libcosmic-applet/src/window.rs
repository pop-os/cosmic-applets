use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4_macros::view;

static STYLE: &str = "
window.cosmic_applet_window {
        background: transparent;
}
";

#[derive(Default)]
pub struct AppletWindowInner;

#[glib::object_subclass]
impl ObjectSubclass for AppletWindowInner {
    const NAME: &'static str = "CosmicAppletWindow";
    type Type = AppletWindow;
    type ParentType = gtk4::Window;
}

impl ObjectImpl for AppletWindowInner {
    fn constructed(&self, obj: &AppletWindow) {
        let window = || obj;
        view! {
            window() {
                add_css_class: "cosmic_applet_window",
                set_decorated: false,
                set_resizable: false,
                set_width_request: 1,
                set_height_request: 1,
            },
            provider = gtk4::CssProvider {
                load_from_data: STYLE.as_bytes(),
            }
        }
        obj.style_context()
            .add_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

impl WidgetImpl for AppletWindowInner {}
impl WindowImpl for AppletWindowInner {}

glib::wrapper! {
    pub struct AppletWindow(ObjectSubclass<AppletWindowInner>)
        @extends gtk4::Widget, gtk4::Window;
}

impl Default for AppletWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl AppletWindow {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }
}
