// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    fl, wayland::AppRequest, wayland_subscription, wayland_subscription::WorkspacesUpdate,
};
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::{
    Element, Task, app,
    app::Core,
    applet::{menu_button, padded_control},
    cosmic_config::{Config, ConfigSet, CosmicConfigEntry},
    cosmic_theme::Spacing,
    iced::{
        Length, Subscription,
        platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup},
        window::Id,
    },
    iced_widget::{column, row},
    surface, theme,
    widget::{
        container, divider,
        segmented_button::{self, Entity, SingleSelectModel},
        segmented_control, text,
    },
};
use cosmic_comp_config::{CosmicCompConfig, TileBehavior};
use cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2::TilingState;
use cosmic_time::{Timeline, anim, chain, id};
use std::{thread, time::Instant};
use tracing::error;

const ID: &str = "com.system76.CosmicAppletTiling";
const ON: &str = "com.system76.CosmicAppletTiling.On";
const OFF: &str = "com.system76.CosmicAppletTiling.Off";

pub struct Window {
    core: Core,
    popup: Option<Id>,
    timeline: Timeline,
    config: CosmicCompConfig,
    config_helper: Config,
    new_workspace_behavior_model: segmented_button::SingleSelectModel,
    new_workspace_entity: Entity,
    /// may not match the config value if behavior is per-workspace
    autotiled: bool,
    workspace_tx: Option<SyncSender<AppRequest>>,
    tile_windows: id::Toggler,
    active_hint: id::Toggler,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Frame(Instant),
    ToggleTileWindows(chain::Toggler, bool),
    ToggleActiveHint(chain::Toggler, bool),
    MyConfigUpdate(Box<CosmicCompConfig>),
    WorkspaceUpdate(WorkspacesUpdate),
    NewWorkspace(Entity),
    OpenSettings,
    Surface(surface::Action),
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

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let config_helper =
            Config::new("com.system76.CosmicComp", CosmicCompConfig::VERSION).unwrap();
        let mut config = CosmicCompConfig::get_entry(&config_helper).unwrap_or_else(|(errs, c)| {
            for err in errs {
                error!(?err, "Error loading config");
            }
            c
        });

        // Global is removed in favor of per-workspace
        if let Err(err) = config.set_autotile_behavior(&config_helper, TileBehavior::PerWorkspace) {
            error!(?err, "Failed to set autotile behavior to PerWorkspace");
        }

        let mut new_workspace_behavior_model = SingleSelectModel::default();
        let new_workspace_entity = new_workspace_behavior_model
            .insert()
            .text(fl!("tiled"))
            .id();
        let floating = new_workspace_behavior_model
            .insert()
            .text(fl!("floating"))
            .id();
        new_workspace_behavior_model.activate(if config.autotile {
            new_workspace_entity
        } else {
            floating
        });

        let window = Self {
            core,
            popup: None,
            timeline: Default::default(),
            autotiled: config.autotile,
            config,
            config_helper,
            new_workspace_behavior_model,
            new_workspace_entity,
            workspace_tx: None,
            tile_windows: id::Toggler::unique(),
            active_hint: id::Toggler::unique(),
        };
        (window, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));
        Subscription::batch(vec![
            timeline,
            self.core
                .watch_config::<CosmicCompConfig>("com.system76.CosmicComp")
                .map(|u| Message::MyConfigUpdate(Box::new(u.config))),
            wayland_subscription::workspaces().map(Message::WorkspaceUpdate),
        ])
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::State(state) => {
                    self.autotiled = matches!(state, TilingState::TilingEnabled);
                    if self.popup.is_some() {
                        self.timeline
                            .set_chain(if self.autotiled {
                                cosmic_time::chain::Toggler::on(self.tile_windows.clone(), 1.0)
                            } else {
                                cosmic_time::chain::Toggler::off(self.tile_windows.clone(), 1.0)
                            })
                            .start();
                    }
                }
                WorkspacesUpdate::Started(tx) => {
                    self.workspace_tx = Some(tx);
                }
                WorkspacesUpdate::Errored => {
                    error!("Workspaces subscription failed...");
                }
            },
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.timeline = Timeline::default();
                    self.tile_windows = id::Toggler::unique();
                    self.active_hint = id::Toggler::unique();
                    let new_id = Id::unique();
                    self.popup = Some(new_id);
                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        Some((1, 1)),
                        None,
                        None,
                    );

                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Frame(now) => self.timeline.now(now),
            Message::ToggleTileWindows(chain, toggled) => {
                self.timeline.set_chain(chain).start();
                self.autotiled = toggled;

                // set via protocol
                if let Some(tx) = self.workspace_tx.as_ref() {
                    let state = if toggled {
                        TilingState::TilingEnabled
                    } else {
                        TilingState::FloatingOnly
                    };

                    if let Err(err) = tx.send(AppRequest::TilingState(state)) {
                        error!("Failed to send the tiling state update. {err:?}")
                    }
                }
            }
            Message::ToggleActiveHint(chain, toggled) => {
                self.timeline.set_chain(chain).start();
                self.config.active_hint = toggled;

                let helper = self.config_helper.clone();
                thread::spawn(move || {
                    if let Err(err) = helper.set("active_hint", toggled) {
                        error!(?err, "Failed to set active_hint {toggled}");
                    }
                });
            }
            Message::MyConfigUpdate(c) => {
                if c.autotile != self.config.autotile {
                    self.new_workspace_behavior_model
                        .activate_position(if c.autotile { 0 } else { 1 });
                }

                if c.active_hint != self.config.active_hint {
                    if self.popup.is_some() {
                        self.timeline
                            .set_chain(if c.active_hint {
                                cosmic_time::chain::Toggler::on(self.active_hint.clone(), 1.0)
                            } else {
                                cosmic_time::chain::Toggler::off(self.active_hint.clone(), 1.0)
                            })
                            .start();
                    }
                }

                self.config = *c;
            }
            Message::NewWorkspace(e) => {
                let autotile_new = self.new_workspace_entity == e;
                self.config.autotile = autotile_new;
                self.new_workspace_behavior_model.activate(e);
                // set the config autotile behavior
                let helper = self.config_helper.clone();

                if let Some(tx) = self.workspace_tx.as_ref() {
                    let state = if autotile_new {
                        TilingState::TilingEnabled
                    } else {
                        TilingState::FloatingOnly
                    };

                    if let Err(err) = tx.send(AppRequest::DefaultBehavior(state)) {
                        error!("Failed to send the tiling state update. {err:?}")
                    }
                }

                thread::spawn(move || {
                    if let Err(err) = helper.set("autotile", autotile_new) {
                        error!(?err, "Failed to set autotile {autotile_new:?}");
                    }
                });
            }
            Message::OpenSettings => {
                let mut cmd = std::process::Command::new("cosmic-settings");
                cmd.arg("window-management");
                tokio::spawn(cosmic::process::spawn(cmd));
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet
            .icon_button(if self.autotiled { ON } else { OFF })
            .on_press_down(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let Spacing {
            space_xxxs,
            space_xxs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        let new_workspace_behavior_button =
            segmented_control::horizontal(&self.new_workspace_behavior_model)
                .on_activate(Message::NewWorkspace);
        let content_list = column![
            padded_control(container(
                anim!(
                    self.tile_windows,
                    &self.timeline,
                    fl!("tile-current"),
                    self.autotiled,
                    |chain, enable| { Message::ToggleTileWindows(chain, enable) },
                )
                .text_size(14)
                .width(Length::Fill),
            ))
            .width(Length::Fill),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            padded_control(
                column![
                    text::body(fl!("new-workspace")),
                    new_workspace_behavior_button,
                ]
                .spacing(space_xxxs)
            ),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            padded_control(row!(
                text::body(fl!("navigate-windows")).width(Length::Fill),
                text::body(format!("{} + {}", fl!("super"), fl!("arrow-keys"))),
            )),
            padded_control(row!(
                text::body(fl!("move-window")).width(Length::Fill),
                text::body(format!(
                    "{} + {} + {}",
                    fl!("shift"),
                    fl!("super"),
                    fl!("arrow-keys")
                )),
            )),
            padded_control(row!(
                text::body(fl!("toggle-floating-window")).width(Length::Fill),
                text::body(format!("{} + G", fl!("super"))),
            )),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            padded_control(
                anim!(
                    self.active_hint,
                    &self.timeline,
                    fl!("active-hint"),
                    self.config.active_hint,
                    |chain, enable| { Message::ToggleActiveHint(chain, enable) },
                )
                .text_size(14)
                .width(Length::Fill),
            ),
            padded_control(divider::horizontal::default()).padding([space_xxs, space_s]),
            menu_button(text::body(fl!("window-management-settings")))
                .on_press(Message::OpenSettings)
        ]
        .padding([8, 0]);

        self.core.applet.popup_container(content_list).into()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
