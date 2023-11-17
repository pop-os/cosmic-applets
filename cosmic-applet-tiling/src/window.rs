use crate::fl;
use cosmic::app::Core;
use cosmic::applet::{menu_button, padded_control};
use cosmic::cosmic_config::{ConfigGet, ConfigSet};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Length, Limits, Subscription};
use cosmic::iced_core::Alignment;
use cosmic::iced_style::application;
use cosmic::iced_widget::{column, row};
use cosmic::widget::{container, divider, spin_button, text};
use cosmic::{Element, Theme};
use cosmic_time::{anim, chain, id, Timeline};
use once_cell::sync::Lazy;
use std::time::Instant;
use tracing::error;

const ID: &str = "com.system76.CosmicAppletTiling";
//const ON: &str = "com.system76.CosmicAppletTiling.On";
const OFF: &str = "com.system76.CosmicAppletTiling.Off";

static TILE_WINDOWS: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

#[derive(Default)]
pub struct Window {
    core: Core,
    popup: Option<Id>,
    timeline: Timeline,
    id_ctr: u128,
    tile_windows: bool,
    active_hint: spin_button::Model<i32>,
    gaps: spin_button::Model<i32>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Frame(Instant),
    ToggleTileWindows(chain::Toggler, bool),
    HandleActiveHint(spin_button::Message),
    HandleGaps(spin_button::Message),
    ViewAllShortcuts,
    OpenFloatingWindowExceptions,
    OpenWindowManagementSettings,
    Ignore,
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
        let mut gaps = spin_button::Model::default().max(99).min(0).step(1);
        gaps.value = core.system_theme().cosmic().gaps.1 as i32;
        let mut active_hint = spin_button::Model::default().max(99).min(0).step(1);
        active_hint.value = core.system_theme().cosmic().active_hint as i32;
        let window = Self {
            core,
            gaps,
            active_hint,
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
            Message::HandleActiveHint(msg) => {
                match msg {
                    spin_button::Message::Increment => {
                        self.active_hint.update(spin_button::Message::Increment)
                    }
                    spin_button::Message::Decrement => {
                        self.active_hint.update(spin_button::Message::Decrement)
                    }
                };
                let is_dark = self.core.system_theme().cosmic().is_dark;
                let active_hint = self.active_hint.value;
                return Command::perform(
                    async move {
                        let config = if is_dark {
                            cosmic::cosmic_theme::ThemeBuilder::dark_config()
                        } else {
                            cosmic::cosmic_theme::ThemeBuilder::light_config()
                        };
                        let Ok(config) = config else {
                            return;
                        };

                        if let Err(err) = ConfigSet::set(&config, "active_hint", active_hint) {
                            error!(?err, "Error setting active_hint");
                        }

                        let config = if is_dark {
                            cosmic::theme::CosmicTheme::dark_config()
                        } else {
                            cosmic::theme::CosmicTheme::light_config()
                        };
                        let Ok(config) = config else {
                            return;
                        };

                        if let Err(err) = ConfigSet::set(&config, "active_hint", active_hint) {
                            error!(?err, "Error setting active_hint");
                        }
                    },
                    |_| cosmic::app::Message::App(Message::Ignore),
                );
            }
            Message::HandleGaps(msg) => {
                match msg {
                    spin_button::Message::Increment => {
                        self.gaps.update(spin_button::Message::Increment)
                    }
                    spin_button::Message::Decrement => {
                        self.gaps.update(spin_button::Message::Decrement)
                    }
                };
                let is_dark = self.core.system_theme().cosmic().is_dark;
                let gaps = self.gaps.value;
                return Command::perform(
                    async move {
                        let config = if is_dark {
                            cosmic::cosmic_theme::ThemeBuilder::dark_config()
                        } else {
                            cosmic::cosmic_theme::ThemeBuilder::light_config()
                        };
                        let Ok(config) = config else {
                            return;
                        };

                        let Ok(mut c_gaps) = ConfigGet::get::<(u32, u32)>(&config, "gaps") else {
                            error!("Error getting gaps");
                            return;
                        };

                        c_gaps.1 = gaps as u32;

                        if let Err(err) = ConfigSet::set(&config, "gaps", c_gaps) {
                            error!(?err, "Error setting gaps");
                        }

                        let config = if is_dark {
                            cosmic::theme::CosmicTheme::dark_config()
                        } else {
                            cosmic::theme::CosmicTheme::light_config()
                        };
                        let Ok(config) = config else {
                            return;
                        };

                        if let Err(err) = ConfigSet::set(&config, "gaps", c_gaps) {
                            error!(?err, "Error setting gaps");
                        }
                    },
                    |_| cosmic::app::Message::App(Message::Ignore),
                );
            }
            Message::ViewAllShortcuts => println!("View all shortcuts..."),
            Message::OpenFloatingWindowExceptions => println!("Floating window exceptions..."),
            Message::OpenWindowManagementSettings => println!("Window management settings..."),
            Message::Ignore => {}
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
        let cosmic = self.core.system_theme().cosmic();
        let active_hint = cosmic.active_hint;
        let gaps = cosmic.gaps.1;
        let content_list = column![
            padded_control(container(
                anim!(
                    TILE_WINDOWS,
                    &self.timeline,
                    fl!("tile-windows"),
                    self.tile_windows,
                    |chain, enable| { Message::ToggleTileWindows(chain, enable) },
                )
                .text_size(14)
                .width(Length::Fill),
            )),
            padded_control(row!(
                text(fl!("navigate-windows")).size(14).width(Length::Fill),
                text(format!("{} + {}", fl!("super"), fl!("arrow-keys"))).size(14),
            )),
            padded_control(row!(
                text(fl!("move-window")).size(14).width(Length::Fill),
                text(format!(
                    "{} + {} + {}",
                    fl!("shift"),
                    fl!("super"),
                    fl!("arrow-keys")
                ))
                .size(14),
            )),
            padded_control(row!(
                text(fl!("toggle-floating-window"))
                    .size(14)
                    .width(Length::Fill),
                text(format!("{} + G", fl!("super"))).size(14),
            )),
            padded_control(divider::horizontal::default()),
            padded_control(
                row!(
                    text(fl!("active-hint")).size(14).width(Length::Fill),
                    spin_button(active_hint.to_string(), Message::HandleActiveHint),
                )
                .align_items(Alignment::Center),
            ),
            padded_control(
                row!(
                    text(fl!("gaps")).size(14).width(Length::Fill),
                    spin_button(gaps.to_string(), Message::HandleGaps),
                )
                .align_items(Alignment::Center),
            ),
            padded_control(divider::horizontal::default()),
            menu_button(text(fl!("view-all-shortcuts")).size(14))
                .on_press(Message::ViewAllShortcuts),
            menu_button(text(fl!("floating-window-exceptions")).size(14))
                .on_press(Message::OpenFloatingWindowExceptions),
            menu_button(text(fl!("window-management-settings")).size(14))
                .on_press(Message::OpenWindowManagementSettings)
        ]
        .padding([8, 0]);

        self.core.applet.popup_container(content_list).into()
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
