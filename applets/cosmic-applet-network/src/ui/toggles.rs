use crate::widgets::SettingsEntry;
use gtk4::{prelude::*, Orientation, Separator, Switch};

pub fn add_toggles(target: &gtk4::Box) {
    view! {
        airplane_mode = SettingsEntry {
            set_title_markup: "<b>Airplane Mode</b>",
            set_child: airplane_mode_switch = &Switch {}
        }
    }
    view! {
        wifi = SettingsEntry {
            set_title_markup: "<b>WiFi</b>",
            set_child: wifi_switch = &Switch {}
        }
    }

    target.append(&airplane_mode);
    target.append(&Separator::new(Orientation::Horizontal));
    target.append(&wifi);
    target.append(&Separator::new(Orientation::Horizontal));
}
