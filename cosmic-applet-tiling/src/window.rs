use crate::fl;
use cosmic::app::Core;
use cosmic::applet::button_theme;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Length, Limits, Subscription};
use cosmic::iced_core::{Alignment, Color};
use cosmic::iced_style::application;
use cosmic::iced_widget::row;
use cosmic::widget::button::StyleSheet;
use cosmic::widget::{button, container, spin_button, text};
use cosmic::{Element, Theme};
use cosmic_time::{anim, chain, id, Timeline};
use once_cell::sync::Lazy;
use std::time::Instant;

const ID: &str = "com.system76.CosmicAppletTiling";
const ON: &str = "com.system76.CosmicAppletTiling.On";
const OFF: &str = "com.system76.CosmicAppletTiling.Off";

static TILE_WINDOWS: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);
static SHOW_ACTIVE_HINTS: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Default)]
pub struct Window {
    core: Core,
    popup: Option<Id>,
    timeline: Timeline,
    id_ctr: u128,
    tile_windows: bool,
    show_active_hint: bool,
    gaps: spin_button::Model<i32>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Frame(Instant),
    ToggleTileWindows(chain::Toggler, bool),
    ToggleShowActiveHint(chain::Toggler, bool),
    HandleGaps(spin_button::Message),
    ViewAllShortcuts,
    OpenFloatingWindowExceptions,
    OpenWindowManagementSettings,
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
            gaps: spin_button::Model::default().max(99).min(0).step(1),
            ..Default::default()
        };
        (window, Command::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));
        Subscription::batch(vec![timeline])
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
                            .applet
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
            Message::Frame(now) => self.timeline.now(now),
            Message::ToggleTileWindows(chain, toggled) => {
                self.timeline.set_chain(chain).start();
                self.tile_windows = toggled
            }
            Message::ToggleShowActiveHint(chain, toggled) => {
                self.timeline.set_chain(chain).start();
                self.show_active_hint = toggled
            }
            Message::HandleGaps(msg) => match msg {
                spin_button::Message::Increment => {
                    self.gaps.update(spin_button::Message::Increment)
                }
                spin_button::Message::Decrement => {
                    self.gaps.update(spin_button::Message::Decrement)
                }
            },
            Message::ViewAllShortcuts => println!("View all shortcuts..."),
            Message::OpenFloatingWindowExceptions => println!("Floating window exceptions..."),
            Message::OpenWindowManagementSettings => println!("Window management settings..."),
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet
            .icon_button(OFF)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let content_list = cosmic::widget::list_column()
            .padding(0)
            .spacing(5)
            .add(
                container(
                    anim!(
                        TILE_WINDOWS,
                        &self.timeline,
                        fl!("tile-windows"),
                        self.tile_windows,
                        |chain, enable| { Message::ToggleTileWindows(chain, enable) },
                    )
                    .text_size(14)
                    .width(Length::Fill),
                )
                .padding([0, 15, 5, 15]),
            )
            .add(
                row!(
                    text(fl!("navigate-windows")).size(14).width(Length::Fill),
                    text(format!("{} + {}", fl!("super"), fl!("arrow-keys"))).size(14),
                )
                .padding([5, 15, 5, 15]),
            )
            .add(
                row!(
                    text(fl!("move-window")).size(14).width(Length::Fill),
                    text(format!(
                        "{} + {} + {}",
                        fl!("shift"),
                        fl!("super"),
                        fl!("arrow-keys")
                    ))
                    .size(14),
                )
                .padding([5, 15, 5, 15]),
            )
            .add(
                row!(
                    text(fl!("toggle-floating-window"))
                        .size(14)
                        .width(Length::Fill),
                    text(format!("{} + G", fl!("super"))).size(14),
                )
                .padding([5, 15, 5, 15]),
            )
            .add(
                container(
                    button(text(fl!("view-all-shortcuts")).size(14))
                        .width(Length::Fill)
                        .style(popup_button_style())
                        .padding(10)
                        .on_press(Message::ViewAllShortcuts),
                )
                .width(Length::Fill)
                .padding([0, 5, 0, 5]),
            )
            .add(
                container(
                    anim!(
                        SHOW_ACTIVE_HINTS,
                        &self.timeline,
                        fl!("show-active-hint"),
                        self.show_active_hint,
                        |chain, enable| { Message::ToggleShowActiveHint(chain, enable) },
                    )
                    .text_size(14)
                    .width(Length::Fill),
                )
                .padding([5, 15, 5, 15]),
            )
            .add(
                row!(
                    text(fl!("gaps")).size(14).width(Length::Fill),
                    spin_button(self.gaps.value.to_string(), Message::HandleGaps),
                )
                .padding([0, 15, 0, 15])
                .align_items(Alignment::Center),
            )
            .add(
                container(
                    button(text(fl!("floating-window-exceptions")).size(14))
                        .width(Length::Fill)
                        .padding(10)
                        .style(popup_button_style())
                        .on_press(Message::OpenFloatingWindowExceptions),
                )
                .width(Length::Fill)
                .padding([0, 5, 0, 5]),
            )
            .add(
                container(
                    button(text(fl!("window-management-settings")).size(14))
                        .width(Length::Fill)
                        .padding(10)
                        .style(popup_button_style())
                        .on_press(Message::OpenWindowManagementSettings),
                )
                .width(Length::Fill)
                .padding([0, 5, 0, 5]),
            );

        self.core
            .applet
            .popup_container(content_list)
            .padding(1)
            .style(popup_style())
            .into()
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}

fn popup_button_style() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|active, t| cosmic::widget::button::Appearance {
            border_radius: 8.0.into(),
            ..t.active(active, &cosmic::theme::Button::Icon)
        }),
        hovered: Box::new(|hovered, t| cosmic::widget::button::Appearance {
            border_radius: 8.0.into(),
            ..t.hovered(hovered, &cosmic::theme::Button::Text)
        }),
        pressed: Box::new(|pressed, t| cosmic::widget::button::Appearance {
            border_radius: 8.0.into(),
            ..t.pressed(pressed, &cosmic::theme::Button::Text)
        }),
        disabled: Box::new(|t| cosmic::widget::button::Appearance {
            border_radius: 8.0.into(),
            ..t.disabled(&cosmic::theme::Button::Standard)
        }),
    }
}

fn popup_style() -> cosmic::theme::Container {
    cosmic::theme::Container::Custom(Box::new(|theme| {
        cosmic::iced_style::container::Appearance {
            icon_color: Some(theme.cosmic().background.on.into()),
            text_color: Some(theme.cosmic().background.on.into()),
            background: Some(Color::from(theme.cosmic().background.base).into()),
            border_radius: 8.0.into(),
            border_width: 2.0,
            border_color: theme.cosmic().bg_divider().into(),
        }
    }))
}
