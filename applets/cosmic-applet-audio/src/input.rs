use gtk4::{glib::clone, prelude::*, Button, Label, ListBox};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use std::rc::Rc;

use crate::pa::{DeviceInfo, PA};

pub async fn get_inputs(pa: &PA) -> Vec<DeviceInfo> {
    // XXX handle error
    pa.get_source_info_list()
        .await
        .expect("failed to list input devices")
}

pub async fn refresh_default_input(pa: &PA, label: &Label) -> DeviceInfo {
    // XXX handle error
    let default_input = pa
        .get_default_source()
        .await
        .expect("failed to get default input");
    label.set_text(match &default_input.description {
        Some(name) => name.as_str(),
        None => "Input Device",
    });
    default_input
}

pub async fn refresh_input_widgets(pa: &PA, inputs: &ListBox) {
    while let Some(row) = inputs.row_at_index(0) {
        inputs.remove(&row);
    }
    for input in get_inputs(pa).await {
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
                    connect_clicked: clone!(@strong pa => move |_| {
                        pa.set_default_source(&name);
                    })
                }
            }
        }
        inputs.container_add(&item);
    }
}
