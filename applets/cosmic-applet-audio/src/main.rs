// SPDX-License-Identifier: GPL-3.0-or-later

#[macro_use]
extern crate relm4_macros;

mod icons;
mod input;
mod now_playing;
mod output;
mod pa;
use pa::PA;
mod task;
mod volume;
mod volume_scale;
use volume_scale::VolumeScale;

use futures::{channel::mpsc, stream::StreamExt};
use gtk4::{
    gio::ApplicationFlags,
    glib::{self, clone, MainContext, PRIORITY_DEFAULT},
    prelude::*,
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Image, Label, ListBox,
    Orientation, PositionType, Revealer, RevealerTransitionType, Scale, SelectionMode, Separator,
};
use libpulse_binding::{
    context::{
        subscribe::{Facility, InterestMaskSet, Operation},
        FlagSet, State,
    },
    volume::Volume,
};
use mpris2_zbus::metadata::Metadata;
use once_cell::sync::Lazy;
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
    // XXX handle no pulseaudio daemon?
    let pa = PA::new().unwrap();
    let (refresh_output_tx, mut refresh_output_rx) = mpsc::unbounded();
    let (refresh_input_tx, mut refresh_input_rx) = mpsc::unbounded();
    let (now_playing_tx, mut now_playing_rx) = mpsc::unbounded::<Vec<Metadata>>();
    pa
        .set_subscribe_callback(clone!(@strong refresh_output_tx, @strong refresh_input_tx => move |facility, operation, _idx| {
            if !matches!(operation, Some(Operation::Changed)) {
                return;
            }
            match facility {
                Some(Facility::Sink) => {
                    refresh_output_tx.unbounded_send(()).expect("failed to send output refresh message");
                }
                Some(Facility::Source) => {
                    refresh_input_tx.unbounded_send(()).expect("failed to send output refresh message");
                }
                _ => {}
            }
        }));
    pa.set_state_callback(move |pa, state| {
        if state == State::Ready {
            pa.subscribe(InterestMaskSet::SINK | InterestMaskSet::SOURCE);
            refresh_output_tx
                .unbounded_send(())
                .expect("failed to send output refresh message");
            refresh_input_tx
                .unbounded_send(())
                .expect("failed to send output refresh message");
        }
    });
    pa.connect().unwrap(); // XXX unwrap
    view! {
        window = libcosmic_applet::AppletWindow {
            set_application: Some(application),
            set_title: Some("COSMIC Network Applet"),
            #[wrap(Some)]
            set_child: button = &libcosmic_applet::AppletButton {
                set_button_icon_name: "audio-volume-medium-symbolic",
                #[wrap(Some)]
                set_popover_child: window_box = &GtkBox {
                    set_orientation: Orientation::Vertical,
                    set_spacing: 24,
                    append: output_box = &GtkBox {
                        set_orientation: Orientation::Horizontal,
                        set_spacing: 16,
                        append: output_icon = &Image {
                            set_icon_name: Some("audio-speakers-symbolic"),
                        },
                        append: output_volume = &VolumeScale::new(pa.clone(), true) {
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
                        append: input_volume = &VolumeScale::new(pa.clone(), false) {
                            set_format_value_func: |_, value| {
                                format!("{:.0}%", value)
                            },
                            set_value_pos: PositionType::Right,
                            set_hexpand: true
                        }
                    },
                    append = &Separator {
                        set_orientation: Orientation::Horizontal,
                    },
                    append: output_list_box = &GtkBox {
                        set_orientation: Orientation::Vertical,
                        append: current_output_button = &Button {
                            #[wrap(Some)]
                            set_child: current_output = &Label {},
                            connect_clicked[outputs_revealer] => move |_| {
                                outputs_revealer.set_reveal_child(!outputs_revealer.reveals_child());
                            }
                        },
                        append: outputs_revealer = &Revealer {
                            set_transition_type: RevealerTransitionType::SlideDown,
                            #[wrap(Some)]
                            set_child: outputs = &ListBox {
                                set_selection_mode: SelectionMode::None,
                                set_activate_on_single_click: true
                            }
                        }
                    },
                    append = &Separator {
                        set_orientation: Orientation::Horizontal,
                    },
                    append: input_list_box = &GtkBox {
                        set_orientation: Orientation::Vertical,
                        append: current_input_button = &Button {
                            #[wrap(Some)]
                            set_child: current_input = &Label {},
                            connect_clicked[inputs_revealer] => move |_| {
                                inputs_revealer.set_reveal_child(!inputs_revealer.reveals_child());
                            }
                        },
                        append: inputs_revealer = &Revealer {
                            set_transition_type: RevealerTransitionType::SlideDown,
                            #[wrap(Some)]
                            set_child: inputs = &ListBox {
                                set_selection_mode: SelectionMode::None,
                                set_activate_on_single_click: true
                            }
                        }
                    },
                    append = &Separator {
                        set_orientation: Orientation::Horizontal,
                    },
                    append: playing_apps = &ListBox {
                        set_selection_mode: SelectionMode::None,
                    }
                }
            }
        }
    }

    glib::MainContext::default().spawn_local(
        clone!(@weak inputs, @weak current_input, @weak input_volume, @strong pa => async move {
            while let Some(()) = refresh_input_rx.next().await {
                input::refresh_input_widgets(&pa, &inputs).await;
                let default_input = input::refresh_default_input(&pa, &current_input).await;
                volume::update_volume(&default_input, &input_volume);
            }
        }),
    );
    glib::MainContext::default().spawn_local(
        clone!(@weak outputs, @weak current_output, @weak output_volume, @strong pa, @strong button, => async move {
            while let Some(()) = refresh_output_rx.next().await {
                output::refresh_output_widgets(&pa, &outputs);
                let default_output = output::refresh_default_output(&pa, &current_output).await;
                volume::update_volume(&default_output, &output_volume);
                button.set_button_icon_name({
                    let volume = default_output.volume.avg().0 as f64 / Volume::NORMAL.0 as f64;
                    // XXX correct cutoffs?
                    if default_output.mute {
                        "audio-volume-muted"
                    } else if volume > 1.0 {
                        "audio-volume-overamplified-symbolic"
                    } else if volume > 0.66 {
                        "audio-volume-high-symbolic"
                    } else if volume > 0.33 {
                        "audio-volume-medium-symbolic"
                    } else {
                        "audio-volume-low-symbolic"
                    }
                });
            }
        }),
    );
    glib::MainContext::default().spawn_local(clone!(@weak playing_apps => async move {
        while let Some(all_metadata) = now_playing_rx.next().await {
        }
    }));
    window.show();
}
