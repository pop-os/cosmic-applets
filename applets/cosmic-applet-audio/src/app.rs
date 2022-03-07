use gtk4::{
    prelude::*, Box as GtkBox, Image, ListBox, Orientation, PositionType, Revealer,
    RevealerTransitionType, Scale, Separator, Window,
};
use libcosmic_widgets::LabeledItem;
use pulsectl::controllers::types::DeviceInfo;
use relm4::{Component, ComponentParts, Sender};

#[derive(Default)]
pub struct App {
    default_input: Option<DeviceInfo>,
    inputs: Vec<DeviceInfo>,
    default_output: Option<DeviceInfo>,
    outputs: Vec<DeviceInfo>,
}

pub struct Widgets {
    inputs: ListBox,
    outputs: ListBox,
}

pub enum Input {
    Compute,
}

pub enum Output {}

pub enum Command {}

pub enum CmdOut {}

impl Component for App {
    type Command = Command;
    type CommandOutput = CmdOut;
    type Input = Input;
    type Output = Output;
    type InitParams = ();
    type Root = Window;
    type Widgets = Widgets;

    fn init_root() -> Self::Root {
        Window::default()
    }

    fn init_parts(
        _args: Self::InitParams,
        root: &Self::Root,
        input: &Sender<Self::Input>,
        _output: &Sender<Self::Output>,
    ) -> ComponentParts<Self> {
        view! {
            GtkBox {
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
                        set_digits: 0
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
                        set_digits: 0
                    }
                },
                append: _sep = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: output_revealer = &Revealer {
                    set_transition_type: RevealerTransitionType::SlideLeft
                }
            }
        }
    }
}
