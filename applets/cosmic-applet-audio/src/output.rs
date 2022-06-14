use gtk4::{prelude::*, Button, Label, ListBox};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use std::rc::Rc;

use crate::pa::{DeviceInfo, PA};

pub async fn get_outputs(pa: &PA) -> Vec<DeviceInfo> {
    // XXX handle error
    pa.get_sink_info_list()
        .await
        .expect("failed to list output devices")
}

pub async fn refresh_default_output(pa: &PA, label: &Label) -> DeviceInfo {
    // XXX handle error
    let default_output = pa
        .get_default_sink()
        .await
        .expect("failed to get default output");
    label.set_text(match &default_output.description {
        Some(name) => name.as_str(),
        None => "Output Device",
    });
    default_output
}

pub async fn refresh_output_widgets(pa: &PA, outputs: &ListBox) {
    while let Some(row) = outputs.row_at_index(0) {
        outputs.remove(&row);
    }
    for output in get_outputs(pa).await {
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
                        // XXX Need mutable borrow? Is this a problem for async?
                        /*
                        SinkController::create()
                            .expect("failed to create output controller")
                            .set_default_device(&name)
                            .expect("failed to set default device");
                        */
                    }
                }
            }
        }
        outputs.container_add(&item);
    }
}
