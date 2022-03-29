use crate::icons::{parse_desktop_icons, DesktopApplication};
use futures_util::StreamExt;
use libcosmic_widgets::LabeledItem;
use libpulse_binding::{
    context::subscribe::{Facility, InterestMaskSet, Operation},
    volume::Volume,
};
use pulsectl::{
    controllers::{
        types::{ApplicationInfo, DeviceInfo},
        AppControl, DeviceControl, SinkController, SourceController,
    },
    Handler,
};
use relm4::{
    component,
    gtk::{
        self,
        glib::{self, clone},
        prelude::*,
        Align, Box as GtkBox, Button, Image, Label, ListBox, Orientation, PositionType, Revealer,
        RevealerTransitionType, Scale, Separator, Window,
    },
    view, ComponentParts, RelmContainerExt, Sender, SimpleComponent,
};
use std::{collections::HashMap, rc::Rc};
use tracker::track;

pub enum AppInput {
    Inputs,
    Outputs,
    InputVolume,
    OutputVolume,
    NowPlaying,
}

#[track]
pub struct App {
    #[no_eq]
    default_input: Option<DeviceInfo>,
    #[no_eq]
    inputs: Vec<DeviceInfo>,
    #[no_eq]
    default_output: Option<DeviceInfo>,
    #[no_eq]
    outputs: Vec<DeviceInfo>,
    #[no_eq]
    now_playing: Vec<ApplicationInfo>,
    #[do_not_track]
    desktop_icons: HashMap<DesktopApplication, String>,
    #[do_not_track]
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
        let now_playing = output_controller.list_applications().unwrap_or_default();
        let desktop_icons = parse_desktop_icons();
        let handler = Handler::connect("com.system76.cosmic.applets.audio")
            .expect("failed to connect to pulse");
        relm4::spawn_local(clone!(@weak handler.mainloop as main_loop => async move {
            let mut timer = async_io::Timer::interval(std::time::Duration::from_millis(100));
            loop {
                main_loop.borrow_mut().iterate(false);
                timer.next().await;
            }
        }));
        Self {
            default_input,
            inputs,
            default_output,
            outputs,
            now_playing,
            desktop_icons,
            handler,
            tracker: 0,
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

    fn refresh_default_input(&mut self) {
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

    fn refresh_default_output(&mut self) {
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
            if !matches!(operation, Some(Operation::Changed)) {
                return;
            }
            match facility {
                Some(Facility::Sink) => {
                    send!(input, AppInput::OutputVolume);
                    send!(input, AppInput::NowPlaying);
                }
                Some(Facility::Source) => {
                    send!(input, AppInput::InputVolume);
                }
                _ => {}
            }
        })));
        context.subscribe(InterestMaskSet::SINK | InterestMaskSet::SOURCE, |_| {});
    }

    fn refresh_input_list(&mut self) {
        let mut input_controller =
            SourceController::create().expect("failed to create input controller");
        self.set_inputs(input_controller.list_devices().unwrap_or_default());
    }

    fn refresh_input_widgets(&self, widgets: &AppWidgets) {
        while let Some(row) = widgets.inputs.row_at_index(0) {
            widgets.inputs.remove(&row);
        }
        for input in self.get_inputs() {
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
            widgets.inputs.container_add(&item);
        }
    }

    fn refresh_output_list(&mut self) {
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        self.set_outputs(output_controller.list_devices().unwrap_or_default());
    }

    fn refresh_output_widgets(&self, widgets: &AppWidgets) {
        while let Some(row) = widgets.outputs.row_at_index(0) {
            widgets.outputs.remove(&row);
        }
        for output in self.get_outputs() {
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

    fn refresh_now_playing(&mut self) {
        let mut output_controller =
            SinkController::create().expect("failed to create output controller");
        self.set_now_playing(output_controller.list_applications().unwrap_or_default());
    }

    fn refresh_now_playing_widgets(&self, widgets: &AppWidgets) {
        while let Some(row) = widgets.playing_apps.row_at_index(0) {
            widgets.playing_apps.remove(&row);
        }
        for app in self.get_now_playing() {
            let index = app.index;
            let muted = app.mute;
            let icon_name = app
                .proplist
                .get_str("application.icon_name")
                .or_else(|| {
                    app.proplist
                        .get_str("application.name")
                        .and_then(|name| self.desktop_icons.get(&DesktopApplication::Name(name)))
                        .cloned()
                })
                .or_else(|| {
                    app.proplist
                        .get_str("application.process.binary")
                        .and_then(|name| self.desktop_icons.get(&DesktopApplication::Binary(name)))
                        .cloned()
                })
                .unwrap_or_default();

            let name = app.name.clone().unwrap_or_default();
            view! {
                item = GtkBox {
                    set_orientation: Orientation::Horizontal,
                    append: icon = &Image {
                        set_icon_name: Some(&icon_name),
                        set_pixel_size: 24,
                    },
                    append: title = &Label {
                        set_label: &name,
                    },
                    append: media_buttons = &GtkBox {
                        set_halign: Align::End,
                        append: pause_button = &Button {
                            set_child: pause_button_img = Some(&Image) {
                                set_icon_name: Some("media-playback-pause-symbolic"),
                                set_pixel_size: 24,
                            },
                            connect_clicked: move |_| {
                                SinkController::create()
                                    .expect("failed to create output controller")
                                    .set_app_mute(index, !muted)
                                    .expect("failed to (un)mute application");
                            }
                        }
                    }
                }
            }
            widgets.playing_apps.container_add(&item);
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
                        set_value: watch! { model.default_output.as_ref().map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.).unwrap_or(0.) },
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
                        set_value: watch! {
                            model.default_input
                                .as_ref()
                                .map(|info| (info.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.)
                                .unwrap_or(0.)
                        },
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
                            set_selection_mode: gtk::SelectionMode::None,
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
                            set_selection_mode: gtk::SelectionMode::None,
                            set_activate_on_single_click: true
                        }
                    }
                },
                &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append: playing_apps = &ListBox {
                    set_selection_mode: gtk::SelectionMode::None,
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
        _output: &Sender<Self::Output>,
    ) {
        self.reset();
        match msg {
            AppInput::Outputs => {
                self.refresh_output_list();
            }
            AppInput::Inputs => {
                self.refresh_input_list();
            }
            AppInput::InputVolume => {
                self.refresh_default_input();
            }
            AppInput::OutputVolume => {
                self.refresh_default_output();
            }
            AppInput::NowPlaying => {
                self.refresh_now_playing();
            }
        }
    }

    fn pre_view() {
        if model.changed(App::outputs()) {
            model.refresh_output_widgets(widgets);
        }
        if model.changed(App::inputs()) {
            model.refresh_input_widgets(widgets);
        }
        if model.changed(App::now_playing()) {
            model.refresh_now_playing_widgets(widgets);
        }
    }
}
