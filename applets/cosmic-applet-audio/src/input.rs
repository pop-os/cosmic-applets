use gtk4::{prelude::*, Button, Label, ListBox};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use pulsectl::controllers::{types::DeviceInfo, DeviceControl, SourceController};
use std::rc::Rc;

fn get_inputs() -> Vec<DeviceInfo> {
    SourceController::create()
        .expect("failed to create input controller")
        .list_devices()
        .expect("failed to list input devices")
}

pub fn refresh_default_input(label: &Label) {
    let default_input = SourceController::create()
        .expect("failed to create input controller")
        .get_default_device()
        .expect("failed to get default input");
    label.set_text(match &default_input.description {
        Some(name) => name.as_str(),
        None => "Input Device",
    });
}

pub fn refresh_input_widgets(inputs: &ListBox) {
    while let Some(row) = inputs.row_at_index(0) {
        inputs.remove(&row);
    }
    for input in get_inputs() {
        let input = Rc::new(input.clone());
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
        inputs.container_add(&item);
    }
}
