use cosmic_panel_config::config::CosmicPanelConfig;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4_macros::view;

use crate::deref_cell::DerefCell;

static STYLE: &str = "
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
pub struct AppletButtonInner {
    menu_button: DerefCell<gtk4::MenuButton>,
    panel_config: DerefCell<CosmicPanelConfig>,
    popover: DerefCell<gtk4::Popover>,
}

#[glib::object_subclass]
impl ObjectSubclass for AppletButtonInner {
    const NAME: &'static str = "CosmicAppletButton";
    type Type = AppletButton;
    type ParentType = gtk4::Widget;
}

impl ObjectImpl for AppletButtonInner {
    fn constructed(&self, obj: &AppletButton) {
        view! {
            menu_button = gtk4::MenuButton {
                set_parent: obj,
                add_css_class: "cosmic_applet_button",
                set_has_frame: false,
                #[wrap(Some)]
                set_popover: popover = &gtk4::Popover {
                    // TODO: change if it can be positioned correctly?
                    set_has_arrow: false,
                }
            },
            provider = gtk4::CssProvider {
                load_from_data: STYLE.as_bytes(),
            }
        }
        obj.set_layout_manager(Some(&gtk4::BinLayout::new()));
        obj.style_context()
            .add_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);

        self.menu_button.set(menu_button);
        self.popover.set(popover);
        self.panel_config
            .set(CosmicPanelConfig::load_from_env().unwrap_or_default());
    }

    fn dispose(&self, _obj: &AppletButton) {
        self.menu_button.unparent();
    }
}

impl WidgetImpl for AppletButtonInner {
    fn compute_expand(&self, _obj: &AppletButton, hexpand: &mut bool, vexpand: &mut bool) {
        *hexpand = self
            .menu_button
            .compute_expand(gtk4::Orientation::Horizontal);
        *vexpand = self.menu_button.compute_expand(gtk4::Orientation::Vertical);
    }

    fn request_mode(&self, _obj: &AppletButton) -> gtk4::SizeRequestMode {
        self.menu_button.request_mode()
    }
}

impl WindowImpl for AppletButtonInner {}

glib::wrapper! {
    pub struct AppletButton(ObjectSubclass<AppletButtonInner>)
        @extends gtk4::Widget;
}

impl Default for AppletButton {
    fn default() -> Self {
        Self::new()
    }
}

impl AppletButton {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }

    fn inner(&self) -> &AppletButtonInner {
        AppletButtonInner::from_instance(self)
    }

    // TODO: avoid multiple instances?
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
