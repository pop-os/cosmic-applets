// SPDX-License-Identifier: LGPL-3.0-or-later

use gtk4::{prelude::*, Align, Button, Image, Label, Orientation};

pub fn build() -> gtk4::Box {
    view! {
        inner_box = gtk4::Box {
            set_orientation: Orientation::Vertical,
            set_spacing: 5,
            append: lock_screen_button = &Button {
                set_child: lock_screen_box = Some(&gtk4::Box) {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 10,
                    append: lock_screen_icon = &Image {
                        set_icon_name: Some("system-lock-screen-symbolic"),
                    },
                    append: lock_screen_label = &Label {
                        set_label: "Lock Screen",
                        set_halign: Align::Start,
                        set_hexpand: true
                    },
                    append: lock_screen_hotkey_label = &Label {
                        set_label: "Super + Escape",
                        set_halign: Align::End
                    }
                }
            },
            append: log_out_button = &Button {
                set_child: log_out_box = Some(&gtk4::Box) {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 10,
                    append: log_out_icon = &Image {
                        set_icon_name: Some("system-log-out-symbolic"),
                    },
                    append: log_out_label = &Label {
                        set_label: "Log Out",
                        set_halign: Align::Start,
                        set_hexpand: true
                    },
                    append: log_out_hotkey_label = &Label {
                        set_label: "Ctrl + Alt + Delete",
                        set_halign: Align::End
                    }
                }
            }
        }
    }
    inner_box
}
