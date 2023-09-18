use crate::fl;
use cosmic::app::Core;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Length, Limits, Subscription};
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::widget::{column, container, divider, spin_button, text};
use cosmic::{Element, Theme};
use cosmic_time::{anim, chain, id, Timeline};
use once_cell::sync::Lazy;
use std::time::Instant;
use cosmic::iced_widget::row;

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
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet_helper
            .icon_button(OFF)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let content_list = cosmic::widget::column()
            .padding(20)
            .spacing(10)
            .push(
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
                .padding([0, 12]),
            )
            .push(divider::horizontal::light())
            .push(
                column()
                    .push(row!(
                        text(fl!("launcher")).size(14).width(Length::Fill),
                        text(format!("{} + /", fl!("super"))).size(14),
                    ))
                    .push(row!(
                        text(fl!("navigate-windows")).size(14).width(Length::Fill),
                        text(format!("{} + {}", fl!("super"), fl!("arrow-keys"))).size(14),
                    ))
                    .push(row!(
                        text(fl!("toggle-tiling")).size(14).width(Length::Fill),
                        text(format!("{} + Y", fl!("super"))).size(14),
                    ))
                    .spacing(10)
                    .padding([0, 20, 0, 20]),
            )
            .push(divider::horizontal::light())
            .push(
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
                .padding([0, 12]),
            )
            .push(row!(
                text(fl!("gaps")).size(14).width(Length::Fill),
                spin_button(self.gaps.value.to_string(), Message::HandleGaps),
            ).padding([0, 10, 0, 10]));

        self.core.applet_helper.popup_container(content_list).into()
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::app::applet::style())
    }
}
