use gtk4::{
    prelude::*, Box as GtkBox, Button, Image, Label, ListBox, Orientation, PositionType, Revealer,
    RevealerTransitionType, Scale, Separator, Window,
};
use libcosmic_widgets::LabeledItem;
use libpulse_binding::{
    context::subscribe::{Facility, InterestMaskSet, Operation},
    volume::Volume,
};
use pulsectl::{
    controllers::{types::DeviceInfo, DeviceControl, SinkController, SourceController},
    Handler,
};
use relm4::{component, view, ComponentParts, RelmContainerExt, Sender, SimpleComponent};
use std::rc::Rc;

pub enum AppInput {
    Inputs,
    Outputs,
    InputVolume,
    OutputVolume,
}

pub struct App {
    default_input: Option<DeviceInfo>,
    inputs: Vec<DeviceInfo>,
    default_output: Option<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
    handler: Handler,
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
        let handler = Handler::connect("com.system76.cosmic.applets.audio")
            .expect("failed to connect to pulse");
        Self {
            default_input,
            inputs,
            default_output,
            outputs,
            handler,
        }
    }
}

impl App {
    fn get_default_input_name(&self) -> &str {
        match &self.default_input {
            Some(input) => match &input.description {
                Some(name) => name.as_str(),
                None => "Input Device",
            },
            None => "No Input Device",
        }
    }

    fn get_default_output_name(&self) -> &str {
        match &self.default_output {
            Some(output) => match &output.description {
                Some(name) => name.as_str(),
                None => "Output Device",
            },
            None => "No Output Device",
        }
    }

    fn update_default_input(&mut self) {
        let mut input_controller =
            SourceController::create().expect("failed to create input controller");
        self.default_input = match self.default_input.as_ref() {
            Some(input) => match &input.name {
                Some(name) => input_controller.get_device_by_name(name.as_str()).ok(),
                None => input_controller.get_device_by_index(input.index).ok(),
            },
            None => return,
        };
    }

    fn update_default_output(&mut self) {
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        self.default_output = match self.default_output.as_ref() {
            Some(output) => match &output.name {
                Some(name) => output_controller.get_device_by_name(name.as_str()).ok(),
                None => output_controller.get_device_by_index(output.index).ok(),
            },
            None => return,
        };
    }

    fn subscribe_for_updates(&self, input: &Sender<AppInput>) {
        let mut context = self.handler.context.borrow_mut();
        let input = input.clone();
        context.set_subscribe_callback(Some(Box::new(move |facility, operation, _idx| {
            match dbg!(operation) {
                Some(Operation::Changed) => {}
                _ => return,
            }
            match dbg!(facility) {
                Some(Facility::Sink | Facility::SinkInput) => {
                    send!(input, AppInput::OutputVolume);
                }
                Some(Facility::Source | Facility::SourceOutput) => {
                    send!(input, AppInput::InputVolume);
                }
                _ => {}
            }
        })));
        context.subscribe(InterestMaskSet::all(), |success| {
            println!("success: {}", success);
        });
    }

    fn update_inputs(&self, widgets: &AppWidgets) {
        let mut input_controller =
            SourceController::create().expect("failed to create input controller");
        let inputs = input_controller.list_devices().unwrap_or_default();
        while let Some(row) = widgets.inputs.row_at_index(0) {
            widgets.inputs.remove(&row);
        }
        for input in inputs {
            let input = Rc::new(input);
            let name = match &input.name {
                Some(name) => name.to_owned(),
                None => continue, // Why doesn't this have a name? Whatever, it's invalid.
            };
            view! {
                item = LabeledItem {
                    set_title: input.description
                        .as_ref()
                        .unwrap_or(&name),
                    set_child: set_current_input_device = &Button {
                        set_label: "Switch",
                        connect_clicked: move |_| {
                            SourceController::create()
                                .expect("failed to create input controller")
                                .set_default_device(&name)
                                .expect("failed to set default device");
                        }
                    }
                }
            }
            widgets.inputs.container_add(&item);
        }
    }

    fn update_outputs(&self, widgets: &AppWidgets) {
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        let outputs = output_controller.list_devices().unwrap_or_default();
        while let Some(row) = widgets.outputs.row_at_index(0) {
            widgets.outputs.remove(&row);
        }
        for output in outputs {
            let output = Rc::new(output);
            let name = match &output.name {
                Some(name) => name.to_owned(),
                None => continue, // Why doesn't this have a name? Whatever, it's invalid.
            };
            view! {
                item = LabeledItem {
                    set_title: output.description
                        .as_ref()
                        .unwrap_or(&name),
                    set_child: set_current_output_device = &Button {
                        set_label: "Switch",
                        connect_clicked: move |_| {
                            SinkController::create()
                                .expect("failed to create output controller")
                                .set_default_device(&name)
                                .expect("failed to set default device");

                        }
                    }
                }
            }
            widgets.outputs.container_add(&item);
        }
    }
}

#[component(pub)]
impl SimpleComponent for App {
    type Widgets = AppWidgets;
    type InitParams = ();
    type Input = AppInput;
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
                        set_value: model.default_output.as_ref().map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.).unwrap_or(0.),
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
                &GtkBox {
                    set_orientation: Orientation::Vertical,
                    &Button {
                        set_child: current_output = Some(&Label) {
                            set_text: watch! { model.get_default_output_name() }
                        },
                        connect_clicked(input, outputs_revealer) => move |_| {
                            send!(input, AppInput::Outputs);
                            outputs_revealer.set_reveal_child(!outputs_revealer.reveals_child());
                        }
                    },
                    append: outputs_revealer = &Revealer {
                        set_transition_type: RevealerTransitionType::SlideDown,
                        set_child: outputs = Some(&ListBox) {
                            set_selection_mode: gtk4::SelectionMode::None,
                            set_activate_on_single_click: true
                        }
                    }
                },
                &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                &GtkBox {
                    set_orientation: Orientation::Vertical,
                    &Button {
                        set_child: current_input = Some(&Label) {
                            set_text: watch! { model.get_default_input_name() }
                        },
                        connect_clicked(input, inputs_revealer) => move |_| {
                            send!(input, AppInput::Inputs);
                            inputs_revealer.set_reveal_child(!inputs_revealer.reveals_child());
                        }
                    },
                    append: inputs_revealer = &Revealer {
                        set_transition_type: RevealerTransitionType::SlideDown,
                        set_child: inputs = Some(&ListBox) {
                            set_selection_mode: gtk4::SelectionMode::None,
                            set_activate_on_single_click: true
                        }
                    }
                }
            }
        }
    }

    fn init_parts(
        _init_params: Self::InitParams,
        root: &Self::Root,
        input: &Sender<Self::Input>,
        _output: &Sender<Self::Output>,
    ) -> ComponentParts<Self> {
        let model = App::default();
        let widgets = view_output!();
        model.subscribe_for_updates(input);

        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        msg: Self::Input,
        _input: &Sender<Self::Input>,
        _ouput: &Sender<Self::Output>,
    ) {
        match msg {
            AppInput::Outputs => {
                self.outputs.clear();
            }
            AppInput::Inputs => {
                self.inputs.clear();
            }
            AppInput::InputVolume => {
                self.update_default_input();
            }
            AppInput::OutputVolume => {
                self.update_default_output();
            }
        }
    }

    fn pre_view() {
        if self.outputs.is_empty() {
            model.update_outputs(widgets);
        }
        if self.inputs.is_empty() {
            model.update_inputs(widgets);
        }
    }
}
