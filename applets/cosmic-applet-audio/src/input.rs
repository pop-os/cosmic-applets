use gtk4::{prelude::*, Button, Label, ListBox};
use libcosmic_widgets::{relm4::RelmContainerExt, LabeledItem};
use std::rc::Rc;

use crate::pa::{Source, PA};

pub async fn get_inputs(pa: &PA) -> Vec<Source> {
    // XXX handle error
    pa.get_source_info_list()
        .await
        .expect("failed to list input devices")
}

pub async fn refresh_default_input(pa: &PA, label: &Label) -> Source {
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
                    connect_clicked: move |_| {
                        // XXX Need mutable borrow? Is this a problem for async?
                        /*
                        SourceController::create()
                            .expect("failed to create input controller")
                            .set_default_device(&name)
                            .expect("failed to set default device");
                        */
                    }
                }
            }
        }
        inputs.container_add(&item);
    }
}
