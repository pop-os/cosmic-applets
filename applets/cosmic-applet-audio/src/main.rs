// SPDX-License-Identifier: LGPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

mod icons;
mod pa;
mod task;

use gtk4::{
    glib::{self, clone},
    prelude::*,
    Align, Box as GtkBox, Button, Image, Label, ListBox, Orientation, PositionType, Revealer,
    RevealerTransitionType, Scale, SelectionMode, Separator, Window,
};
use once_cell::sync::Lazy;
use pulsectl::Handler;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let handler =
        Handler::connect("com.system76.cosmic.applets.audio").expect("failed to connect to pulse");
    task::spawn_local(clone!(@strong handler.mainloop as main_loop => async move {
        pa::drive_main_loop(main_loop).await
    }));
    view! {
        window = Window {
            set_title: Some("COSMIC Network Applet"),
            set_default_width: 400,
            set_default_height: 300,

            set_child: window_box = Some(&GtkBox) {
                set_orientation: Orientation::Vertical,
                set_spacing: 24,
                append: output_box = &GtkBox {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 16,
                    append: output_icon = &Image {
                        set_icon_name: Some("audio-speakers-symbolic"),
                    },
                    append: output_volume = &Scale::with_range(Orientation::Horizontal, 0., 100., 1.) {
                        set_format_value_func: |_, value| {
                            format!("{:.0}%", value)
                        },
                        //set_value: watch! { model.default_output.as_ref().map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.).unwrap_or(0.) },
                        set_value_pos: PositionType::Right,
                        set_hexpand: true
                    }
                },
                append: input_box = &GtkBox {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 16,
                    append: input_icon = &Image {
                        set_icon_name: Some("audio-input-microphone-symbolic"),
                    },
                    append: input_volume = &Scale::with_range(Orientation::Horizontal, 0., 100., 1.) {
                        set_format_value_func: |_, value| {
                            format!("{:.0}%", value)
                        },
                        /*set_value: watch! {
                            model.default_input
                                .as_ref()
                                .map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.)
                                .unwrap_or(0.)
                        },*/
                        set_value_pos: PositionType::Right,
                        set_hexpand: true
                    }
                },
                append: _sep = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: output_list_box = &GtkBox {
                    set_orientation: Orientation::Vertical,
                    append: current_output_button = &Button {
                        set_child: current_output = Some(&Label) {},
                        connect_clicked(outputs_revealer) => move |_| {
                            outputs_revealer.set_reveal_child(!outputs_revealer.reveals_child());
                        }
                    },
                    append: outputs_revealer = &Revealer {
                        set_transition_type: RevealerTransitionType::SlideDown,
                        set_child: outputs = Some(&ListBox) {
                            set_selection_mode: SelectionMode::None,
                            set_activate_on_single_click: true
                        }
                    }
                },
                append: _sep = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: input_list_box = &GtkBox {
                    set_orientation: Orientation::Vertical,
                    append: current_input_button = &Button {
                        set_child: current_input = Some(&Label) {},
                        connect_clicked(inputs_revealer) => move |_| {
                            inputs_revealer.set_reveal_child(!inputs_revealer.reveals_child());
                        }
                    },
                    append: inputs_revealer = &Revealer {
                        set_transition_type: RevealerTransitionType::SlideDown,
                        set_child: inputs = Some(&ListBox) {
                            set_selection_mode: SelectionMode::None,
                            set_activate_on_single_click: true
                        }
                    }
                },
                append: _sep = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: playing_apps = &ListBox {
                    set_selection_mode: SelectionMode::None,
                }
            }
        }
    }
}
