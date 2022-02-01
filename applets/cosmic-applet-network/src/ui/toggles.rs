use gtk4::{prelude::*, Align, Label, Orientation, Separator, Switch};

pub fn add_toggles(target: &gtk4::Box) {
    view! {
        airplane_mode_box = gtk4::Box {
            append: airplane_mode_label = &Label {
                set_markup: "<b>Airplane Mode</b>",
                set_halign: Align::Start
            },
            append: airplane_mode_switch = &Switch {}
        }
    }
    view! {
        wifi_box = gtk4::Box {
            append: wifi_label = &Label {
                set_markup: "<b>WiFi</b>",
                set_halign: Align::Start
            },
            append: wifi_switch = &Switch {}
        }
    }

    target.append(&airplane_mode_box);
    target.append(&Separator::new(Orientation::Horizontal));
    target.append(&wifi_box);
    target.append(&Separator::new(Orientation::Horizontal));
}
