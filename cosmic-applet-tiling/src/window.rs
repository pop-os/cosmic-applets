use crate::fl;
use cosmic::app::Core;
use cosmic::cosmic_theme::palette::rgb::Rgb;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Limits};
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::theme::{Button, Svg};
use cosmic::widget::{button, list_column, settings, spin_button, text, toggler};
use cosmic::{Element, Theme};

const ID: &str = "com.system76.CosmicAppletTiling";

#[derive(Default)]
pub struct Window {
    core: Core,
    popup: Option<Id>,
    id_ctr: u128,
    tile_windows: bool,
    show_window_titles: bool,
    show_active_hint: bool,
    active_border_radius: spin_button::Model<i32>,
    active_hint_color: Rgb,
    gaps: spin_button::Model<i32>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    ToggleTileWindows(bool),
    ToggleShowWindowTitles(bool),
    ToggleShowActiveHint(bool),
    HandleActiveBorderRadius(spin_button::Message),
    SetActiveHintColor(Rgb),
    HandleGaps(spin_button::Message),
}

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        _flags: Self::Flags,
    ) -> (Self, Command<cosmic::app::Message<Self::Message>>) {
        let window = Window {
            core,
            active_border_radius: spin_button::Model::default().max(99).min(0).step(1),
            gaps: spin_button::Model::default().max(99).min(0).step(1),
            ..Default::default()
        };
        (window, Command::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.id_ctr += 1;
                    let new_id = Id(self.id_ctr);
                    self.popup.replace(new_id);
                    let mut popup_settings =
                        self.core
                            .applet_helper
                            .get_popup_settings(Id(0), new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::ToggleTileWindows(toggled) => self.tile_windows = toggled,
            Message::ToggleShowWindowTitles(toggled) => self.show_window_titles = toggled,
            Message::ToggleShowActiveHint(toggled) => self.show_active_hint = toggled,
            Message::HandleActiveBorderRadius(msg) => match msg {
                spin_button::Message::Increment => self
                    .active_border_radius
                    .update(spin_button::Message::Increment),
                spin_button::Message::Decrement => self
                    .active_border_radius
                    .update(spin_button::Message::Decrement),
            },
            Message::SetActiveHintColor(_) => {}
            Message::HandleGaps(msg) => match msg {
                spin_button::Message::Increment => {
                    self.gaps.update(spin_button::Message::Increment)
                }
                spin_button::Message::Decrement => {
                    self.gaps.update(spin_button::Message::Decrement)
                }
            },
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet_helper
            .icon_button(ID)
            .on_press(Message::TogglePopup)
            .style(Button::Text)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let content_list = list_column()
            .add(settings::item(
                fl!("tile-windows"),
                toggler(None, self.tile_windows, |value| {
                    Message::ToggleTileWindows(value)
                }),
            ))
            .add(settings::item(
                fl!("floating-window-exceptions"),
                button(Button::Card).icon(Svg::Symbolic, "arrow-right", 16),
            ))
            .add(
                settings::view_section(fl!("shortcuts"))
                    .add(settings::item(
                        fl!("launcher"),
                        text(format!("{} + /", fl!("super"))),
                    ))
                    .add(settings::item(
                        fl!("navigate-windows"),
                        text(format!("{} + {}", fl!("super"), fl!("arrow-keys"))),
                    ))
                    .add(settings::item(
                        fl!("toggle-tiling"),
                        text(format!("{} + Y", fl!("super"))),
                    ))
                    .add(settings::item(fl!("view-all"), text(""))),
            )
            .add(settings::item(
                fl!("show-window-titles"),
                toggler(None, self.show_window_titles, |value| {
                    Message::ToggleShowWindowTitles(value)
                }),
            ))
            .add(settings::item(
                fl!("show-active-hint"),
                toggler(None, self.show_active_hint, |value| {
                    Message::ToggleShowActiveHint(value)
                }),
            ))
            .add(settings::item(
                fl!("active-border-radius"),
                spin_button(
                    self.active_border_radius.value.to_string(),
                    Message::HandleActiveBorderRadius,
                ),
            ))
            .add(settings::item(fl!("active-hint-color"), text("TODO")))
            .add(settings::item(
                fl!("gaps"),
                spin_button(self.gaps.value.to_string(), Message::HandleGaps),
            ));

        self.core.applet_helper.popup_container(content_list).into()
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::app::applet::style())
    }
}
