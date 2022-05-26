// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

mod icons;
mod input;
mod now_playing;
mod output;
mod pa;
mod task;
mod volume;

use gtk4::{
    gio::ApplicationFlags,
    glib::{self, clone, MainContext, PRIORITY_DEFAULT},
    prelude::*,
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Image, Label, ListBox,
    Orientation, PositionType, Revealer, RevealerTransitionType, Scale, SelectionMode, Separator,
};
use libpulse_binding::{
    context::subscribe::{Facility, InterestMaskSet, Operation},
    volume::Volume,
};
use mpris2_zbus::metadata::Metadata;
use once_cell::sync::Lazy;
use pulsectl::Handler;
use tokio::runtime::Runtime;

static RT: Lazy<Runtime> = Lazy::new(|| Runtime::new().expect("failed to build tokio runtime"));

fn main() {
    let application = Application::new(
        Some("com.system76.cosmic.applets.audio"),
        ApplicationFlags::default(),
    );
    application.connect_activate(app);
    application.run();
}

fn app(application: &Application) {
    let handler =
        Handler::connect("com.system76.cosmic.applets.audio").expect("failed to connect to pulse");
    task::spawn_local(clone!(@strong handler.mainloop as main_loop => async move {
        pa::drive_main_loop(main_loop).await
    }));
    let (refresh_output_tx, refresh_output_rx) = MainContext::channel::<()>(PRIORITY_DEFAULT);
    let (refresh_input_tx, refresh_input_rx) = MainContext::channel::<()>(PRIORITY_DEFAULT);
    let (now_playing_tx, now_playing_rx) = MainContext::channel::<Vec<Metadata>>(PRIORITY_DEFAULT);
    handler
        .context
        .borrow_mut()
        .set_subscribe_callback(Some(Box::new(clone!(@strong refresh_output_tx, @strong refresh_input_tx => move |facility, operation, _idx| {
            if !matches!(operation, Some(Operation::Changed)) {
                return;
            }
            match facility {
                Some(Facility::Sink) => {
                    refresh_output_tx.send(()).expect("failed to send output refresh message");
                }
                Some(Facility::Source) => {
                    refresh_input_tx.send(()).expect("failed to send output refresh message");
                }
                _ => {}
            }
        }))));
    handler
        .context
        .borrow_mut()
        .subscribe(InterestMaskSet::SINK | InterestMaskSet::SOURCE, |_| {});
    view! {
        window = ApplicationWindow {
            set_application: Some(application),
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
    refresh_input_rx.attach(
        None,
        clone!(@weak inputs, @weak current_input, @weak input_volume => @default-return Continue(true), move |_| {
            input::refresh_input_widgets(&inputs);
            let default_input = input::refresh_default_input(&current_input);
            volume::update_volume(&default_input, &input_volume);
            Continue(true)
        }),
    );
    refresh_output_rx.attach(
        None,
        clone!(@weak outputs, @weak current_output, @weak output_volume => @default-return Continue(true), move |_| {
            output::refresh_output_widgets(&outputs);
            let default_output = output::refresh_default_output(&current_output);
            volume::update_volume(&default_output, &output_volume);
            Continue(true)
        }),
    );
    now_playing_rx.attach(
        None,
        clone!(@weak playing_apps => @default-return Continue(true), move |all_metadata| {
            Continue(true)
        }),
    );
    window.show();
}
