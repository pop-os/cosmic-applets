use cosmic_panel_config::CosmicPanelConfig;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use relm4_macros::view;

mod button;
pub use button::AppletButton;
mod deref_cell;
mod window;
pub use window::AppletWindow;

// TODO make sure style fits different panel colors?
// TODO abstraction to start main loop? Work with relm4.
// TODO gir bindings
// TODO orientation, etc.
// TODO make image size dependent on CosmicPanelConfig?
// TODO way to have multiple applets with this style, for system tray.
// TODO also handle non-popover button? Is GtkMenuButton particularly special, or just use a toggle button?
