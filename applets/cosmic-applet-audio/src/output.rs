use gtk4::{prelude::*, Button, Label, ListBox};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use pulsectl::controllers::{types::DeviceInfo, DeviceControl, SinkController};
use std::rc::Rc;

fn get_outputs() -> Vec<DeviceInfo> {
    SinkController::create()
        .expect("failed to create output controller")
        .list_devices()
        .expect("failed to list output devices")
}

pub fn refresh_default_output(label: &Label) -> DeviceInfo {
    let default_output = SinkController::create()
        .expect("failed to create output controller")
        .get_default_device()
        .expect("failed to get default output");
    label.set_text(match &default_output.description {
        Some(name) => name.as_str(),
        None => "Output Device",
    });
    default_output
}

pub fn refresh_output_widgets(outputs: &ListBox) {
    while let Some(row) = outputs.row_at_index(0) {
        outputs.remove(&row);
    }
    for output in get_outputs() {
        let output = Rc::new(output.clone());
        let name = match &output.name {
            Some(name) => name.to_owned(),
            None => continue, // Why doesn't this have a name? Whatever, it's invalid.
        };
        view! {
            item = LabeledItem {
                set_title: output.description
                    .as_ref()
                    .unwrap_or(&name),
                set_child: set_current_input_device = &Button {
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
        outputs.container_add(&item);
    }
}
