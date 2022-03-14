use gtk4::{
    glib::{self, clone},
    prelude::*,
    Box as GtkBox, Button, Image, Label, ListBox, Orientation, PositionType, Scale, Separator,
    Stack, Window,
};
use libcosmic_widgets::LabeledItem;
use libpulse_binding::volume::Volume;
use pulsectl::controllers::{types::DeviceInfo, DeviceControl, SinkController, SourceController};
use relm4::{component, Component, ComponentParts, Sender, SimpleComponent};
use std::rc::Rc;

pub struct App {
    default_input: Option<DeviceInfo>,
    inputs: Vec<DeviceInfo>,
    default_output: Option<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
}

impl Default for App {
    fn default() -> Self {
        let mut input_controller =
            SourceController::create().expect("failed to create input controller");
        let default_input = input_controller.get_default_device().ok();
        let inputs = input_controller.list_devices().unwrap_or_default();
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        let default_output = output_controller.get_default_device().ok();
        let outputs = output_controller.list_devices().unwrap_or_default();
        Self {
            default_input,
            inputs,
            default_output,
            outputs,
        }
    }
}

impl App {
    pub fn get_default_input_name(&self) -> &str {
        match &self.default_input {
            Some(input) => match &input.description {
                Some(name) => name.as_str(),
                None => "Input Device",
            },
            None => "No Input Device",
        }
    }

    pub fn get_default_output_name(&self) -> &str {
        match &self.default_output {
            Some(output) => match &output.description {
                Some(name) => name.as_str(),
                None => "Output Device",
            },
            None => "No Output Device",
        }
    }
}

pub enum Input {}

impl App {
    pub fn update_outputs(&self, widgets: &mut AppWidgets) {
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        let outputs = output_controller.list_devices().unwrap_or_default();
        while let Some(row) = widgets.outputs.row_at_index(1) {
            widgets.outputs.remove(&row);
        }
        for output in outputs {
            let output = Rc::new(output);
            view! {
                item = LabeledItem {
                    set_title: output.description
                        .as_ref()
                        .or_else(|| output.name.as_ref())
                        .cloned()
                        .unwrap_or_else(|| "Unknown".to_string()),
                    set_child: set_current_output_device = &Button {
                        set_label: "Switch",
                        connect_clicked: clone!(@strong output, => move |_| {
                            if let Some(name) = &output.name {
                                SinkController::create()
                                    .expect("failed to create output controller")
                                    .set_default_device(name)
                                    .expect("failed to set default device");
                            }
                        })
                    }
                }
            }
        }
    }
}

#[component(pub)]
impl SimpleComponent for App {
    type Widgets = AppWidgets;
    type InitParams = ();
    type Input = Input;
    type Output = ();

    view! {
        Window {
            set_title: Some("COSMIC Network Applet"),
            set_default_width: 400,
            set_default_height: 300,

            &GtkBox {
                set_orientation: Orientation::Vertical,
                set_spacing: 24,
                &GtkBox {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 16,
                    &Image {
                        set_icon_name: Some("audio-speakers-symbolic"),
                    },
                    append: output_volume = &Scale::with_range(Orientation::Horizontal, 0., 100., 1.) {
                        set_format_value_func: |_, value| {
                            format!("{:.0}%", value)
                        },
                        set_value: model.default_output.as_ref().map(|info| dbg!((info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.)).unwrap_or(0.),
                        set_value_pos: PositionType::Right,
                        set_hexpand: true
                    }
                },
                &GtkBox {
                    set_orientation: Orientation::Horizontal,
                    set_spacing: 16,
                    &Image {
                        set_icon_name: Some("audio-input-microphone-symbolic"),
                    },
                    append: input_volume = &Scale::with_range(Orientation::Horizontal, 0., 100., 1.) {
                        set_format_value_func: |_, value| {
                            format!("{:.0}%", value)
                        },
                        set_value: model.default_input
                            .as_ref()
                            .map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.)
                            .unwrap_or(0.),
                        set_value_pos: PositionType::Right,
                        set_hexpand: true
                    }
                },
                &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: output_stack = &Stack {
                    add_child: outputs = &ListBox {
                        set_selection_mode: gtk4::SelectionMode::None,
                        set_activate_on_single_click: true
                    },
                    add_child: open_outputs_button = &Button {
                        set_child: current_output = Some(&Label) {
                            set_text: watch! { model.get_default_output_name() }
                        },
                    }
                },
                &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: input_stack = &Stack {
                    add_child: inputs = &ListBox {
                        set_selection_mode: gtk4::SelectionMode::None,
                        set_activate_on_single_click: true
                    },
                    add_child: open_inputs_button = &Button {
                        set_child: current_input = Some(&Label) {
                            set_text: watch! { model.get_default_input_name() }
                        }
                    }
                }
            }
        }
    }

    fn init_parts(
        _init_params: Self::InitParams,
        root: &Self::Root,
        _input: &Sender<Self::Input>,
        _output: &Sender<Self::Output>,
    ) -> ComponentParts<Self> {
        let model = App::default();
        let widgets = view_output!();
        widgets
            .output_stack
            .set_visible_child(&widgets.open_outputs_button);
        widgets
            .input_stack
            .set_visible_child(&widgets.open_inputs_button);

        ComponentParts { model, widgets }
    }
}
