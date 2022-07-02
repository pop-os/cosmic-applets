use cosmic_panel_config::config::CosmicPanelConfig;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4_macros::view;

mod deref_cell;
use deref_cell::DerefCell;

// TODO make sure style fits different panel colors?
// TODO abstraction to start main loop? Work with relm4.
// TODO gir bindings
// TODO orientation, etc.
// TODO make image size dependent on CosmicPanelConfig?
// TODO way to have multiple applets with this style, for system tray.

static STYLE: &str = "
window.cosmic_applet_window {
        background: transparent;
}

button.cosmic_applet_button {
    border-radius: 12px;
    transition: 100ms;
    padding: 4px;
    border-color: transparent;
    background: transparent;
    outline-color: transparent;
}
";

#[derive(Default)]
pub struct AppletInner {
    panel_config: DerefCell<CosmicPanelConfig>,
    menu_button: DerefCell<gtk4::MenuButton>,
    popover: DerefCell<gtk4::Popover>,
}

#[glib::object_subclass]
impl ObjectSubclass for AppletInner {
    const NAME: &'static str = "CosmicApplet";
    type Type = Applet;
    type ParentType = gtk4::Window;
}

impl ObjectImpl for AppletInner {
    fn constructed(&self, obj: &Applet) {
        let window = || obj;
        view! {
            window() {
                add_css_class: "cosmic_applet_window",
                set_decorated: false,
                set_resizable: false,
                set_width_request: 1,
                set_height_request: 1,
                #[wrap(Some)]
                set_child: menu_button = &gtk4::MenuButton {
                    add_css_class: "cosmic_applet_button",
                    set_has_frame: false,
                    #[wrap(Some)]
                    set_popover: popover = &gtk4::Popover {
                        // TODO: change if it can be positioned correctly?
                        set_has_arrow: false,
                    }
                }
            }
        }

        let provider = gtk4::CssProvider::new();
        provider.load_from_data(STYLE.as_bytes());
        obj.style_context()
            .add_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

        self.menu_button.set(menu_button);
        self.popover.set(popover);
        self.panel_config
            .set(CosmicPanelConfig::load_from_env().unwrap_or_default());
    }
}

impl WidgetImpl for AppletInner {}
impl WindowImpl for AppletInner {}

glib::wrapper! {
    pub struct Applet(ObjectSubclass<AppletInner>)
        @extends gtk4::Widget, gtk4::Window;
}

impl Default for Applet {
    fn default() -> Self {
        Self::new()
    }
}

impl Applet {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &AppletInner {
        AppletInner::from_instance(self)
    }

    pub fn panel_config(&self) -> &CosmicPanelConfig {
        &*self.inner().panel_config
    }

    pub fn set_button_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
        self.inner().menu_button.set_child(child);
    }

    pub fn set_button_icon_name(&self, name: &str) {
        let image = gtk4::Image::from_icon_name(name);
        image.set_pixel_size(
            self.panel_config()
                .get_applet_icon_size()
                .try_into()
                .unwrap(),
        ); // XXX unwrap
        self.set_button_child(Some(&image));
    }

    pub fn set_button_label(&self, label: &str) {
        self.inner().menu_button.set_label(label);
    }

    pub fn set_popover_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
        self.inner().popover.set_child(child);
    }
}
