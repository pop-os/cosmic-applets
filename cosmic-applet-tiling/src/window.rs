use crate::wayland_subscription::WorkspacesUpdate;
use crate::{fl, wayland_subscription};
use cctk::sctk::reexports::calloop::channel::SyncSender;
use cosmic::app::Core;
use cosmic::applet::padded_control;
use cosmic::cosmic_config::{Config, ConfigSet, CosmicConfigEntry};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Length, Limits, Subscription};
use cosmic::iced_style::application;
use cosmic::iced_widget::{column, row};
use cosmic::widget::segmented_button::{Entity, SingleSelectModel};
use cosmic::widget::{
    container, divider, segmented_button, segmented_selection, spin_button, text,
};
use cosmic::{Element, Theme};
use cosmic_comp_config::{CosmicCompConfig, TileBehavior};
use cosmic_protocols::workspace::v1::client::zcosmic_workspace_handle_v1::TilingState;
use cosmic_time::{anim, chain, id, Timeline};
use once_cell::sync::Lazy;
use std::thread;
use std::time::Instant;
use tracing::error;

const ID: &str = "com.system76.CosmicAppletTiling";
const OFF: &str = "com.system76.CosmicAppletTiling.Off";

static TILE_WINDOWS: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);
static ACTIVE_HINT: Lazy<id::Toggler> = Lazy::new(id::Toggler::unique);

pub struct Window {
    core: Core,
    popup: Option<Id>,
    timeline: Timeline,
    config: CosmicCompConfig,
    config_helper: Config,
    autotile_behavior_model: segmented_button::SingleSelectModel,
    new_workspace_behavior_model: segmented_button::SingleSelectModel,
    autotile_global_entity: Entity,
    new_workspace_entity: Entity,
    /// may not match the config value if behavior is per-workspace
    autotiled: bool,
    workspace_tx: Option<SyncSender<TilingState>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Frame(Instant),
    ToggleTileWindows(chain::Toggler, bool),
    ToggleActiveHint(chain::Toggler, bool),
    MyConfigUpdate(CosmicCompConfig),
    TileMode(Entity),
    WorkspaceUpdate(WorkspacesUpdate),
    NewWorkspace(Entity),
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
        let config_helper =
            Config::new("com.system76.CosmicComp", CosmicCompConfig::VERSION).unwrap();
        let config = CosmicCompConfig::get_entry(&config_helper).unwrap_or_else(|(errs, c)| {
            for err in errs {
                error!(?err, "Error loading config");
            }
            c
        });
        let mut autotile_behavior_model = SingleSelectModel::default();
        let autotile_global_entity = autotile_behavior_model
            .insert()
            .text(fl!("all-workspaces"))
            .id();
        let per = autotile_behavior_model
            .insert()
            .text(fl!("per-workspace"))
            .id();
        autotile_behavior_model.activate(match config.autotile_behavior {
            TileBehavior::Global => autotile_global_entity,
            TileBehavior::PerWorkspace => per,
        });

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
            autotile_behavior_model,
            new_workspace_behavior_model,
            autotile_global_entity,
            new_workspace_entity,
            workspace_tx: None,
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
        Subscription::batch(vec![
            timeline,
            self.core
                .watch_config::<CosmicCompConfig>("com.system76.CosmicComp")
                .map(|u| Message::MyConfigUpdate(u.config)),
            wayland_subscription::workspaces().map(|e| Message::WorkspaceUpdate(e)),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::State(state) => {
                    self.autotiled = matches!(state, TilingState::TilingEnabled);
                    self.timeline.set_chain(if self.autotiled {
                        cosmic_time::chain::Toggler::on(TILE_WINDOWS.clone(), 1.0)
                    } else {
                        cosmic_time::chain::Toggler::off(TILE_WINDOWS.clone(), 1.0)
                    });
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
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings =
                        self.core
                            .applet
                            .get_popup_settings(Id::MAIN, new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(400.0)
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
                self.autotiled = toggled;
                if matches!(self.config.autotile_behavior, TileBehavior::Global) {
                    self.config.autotile = toggled;
                    if toggled {
                        self.new_workspace_behavior_model.activate_position(0);
                    } else {
                        self.new_workspace_behavior_model.activate_position(1);
                    }
                    let helper = self.config_helper.clone();
                    thread::spawn(move || {
                        if let Err(err) = helper.set("autotile", toggled) {
                            error!(?err, "Failed to set autotile {toggled}");
                        }
                    });
                } else {
                    // set via protocol
                    if let Some(tx) = self.workspace_tx.as_ref() {
                        let state = if toggled {
                            TilingState::TilingEnabled
                        } else {
                            TilingState::FloatingOnly
                        };

                        if let Err(err) = tx.send(state) {
                            error!("Failed to send the tiling state update. {err:?}")
                        }
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
                if matches!(c.autotile_behavior, TileBehavior::Global) {
                    if c.autotile != self.config.autotile {
                        self.timeline.set_chain(if c.autotile {
                            cosmic_time::chain::Toggler::on(TILE_WINDOWS.clone(), 1.0)
                        } else {
                            cosmic_time::chain::Toggler::off(TILE_WINDOWS.clone(), 1.0)
                        });
                    }
                    self.autotile_behavior_model.activate_position(0);
                } else {
                    if c.autotile != self.config.autotile {
                        self.new_workspace_behavior_model
                            .activate_position(if c.autotile { 0 } else { 1 });
                    }
                    self.autotile_behavior_model.activate_position(1);
                }
                if c.active_hint != self.config.active_hint {
                    self.timeline.set_chain(if c.active_hint {
                        cosmic_time::chain::Toggler::on(ACTIVE_HINT.clone(), 1.0)
                    } else {
                        cosmic_time::chain::Toggler::off(ACTIVE_HINT.clone(), 1.0)
                    });
                }

                self.config = c;
            }
            Message::TileMode(e) => {
                let behavior = if e == self.autotile_global_entity {
                    TileBehavior::Global
                } else {
                    TileBehavior::PerWorkspace
                };

                self.config.autotile_behavior = behavior;
                self.autotile_behavior_model.activate(e);
                let helper = self.config_helper.clone();
                let need_to_reset_autotile = self.autotiled != self.config.autotile
                    && matches!(behavior, TileBehavior::Global);
                if need_to_reset_autotile {
                    self.autotiled = self.config.autotile;
                }
                thread::spawn(move || {
                    if let Err(err) = helper.set("autotile_behavior", behavior) {
                        error!(?err, "Failed to set autotile_behavior {behavior:?}");
                    }
                });
            }
            Message::NewWorkspace(e) => {
                let autotile_new = self.new_workspace_entity == e;
                self.config.autotile = autotile_new;
                self.new_workspace_behavior_model.activate(e);
                // set the config autotile behavior
                let helper = self.config_helper.clone();
                thread::spawn(move || {
                    if let Err(err) = helper.set("autotile", autotile_new) {
                        error!(?err, "Failed to set autotile {autotile_new:?}");
                    }
                });
            }
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
        let space_xxs = self.core.system_theme().cosmic().space_xxs();
        let mut new_workspace_behavior_button =
            segmented_selection::horizontal(&self.new_workspace_behavior_model);
        if matches!(self.config.autotile_behavior, TileBehavior::PerWorkspace) {
            new_workspace_behavior_button =
                new_workspace_behavior_button.on_activate(|e| Message::NewWorkspace(e));
        }
        let content_list = column![
            padded_control(container(
                anim!(
                    TILE_WINDOWS,
                    &self.timeline,
                    if matches!(self.config.autotile_behavior, TileBehavior::Global) {
                        fl!("tile-windows")
                    } else {
                        fl!("tile-current")
                    },
                    self.autotiled,
                    |chain, enable| { Message::ToggleTileWindows(chain, enable) },
                )
                .text_size(14)
                .width(Length::Fill),
            ))
            .width(Length::Fill),
            padded_control(
                column![
                    divider::horizontal::default(),
                    column![
                        text(fl!("autotile-behavior")).size(14),
                        segmented_selection::horizontal(&self.autotile_behavior_model)
                            .on_activate(|e| Message::TileMode(e))
                    ],
                    divider::horizontal::default(),
                    column![
                        text(fl!("new-workspace")).size(14),
                        new_workspace_behavior_button,
                    ]
                ]
                .spacing(space_xxs)
            ),
            padded_control(divider::horizontal::default()),
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
                anim!(
                    ACTIVE_HINT,
                    &self.timeline,
                    fl!("active-hint"),
                    self.config.active_hint,
                    |chain, enable| { Message::ToggleActiveHint(chain, enable) },
                )
                .text_size(14)
                .width(Length::Fill),
            ),
        ]
        .padding([8, 0]);

        self.core.applet.popup_container(content_list).into()
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
