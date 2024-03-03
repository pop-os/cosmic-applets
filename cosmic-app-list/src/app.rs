use crate::config;
use crate::config::AppListConfig;
use crate::config::APP_ID;
use crate::fl;
use crate::wayland_subscription::wayland_subscription;
use crate::wayland_subscription::ToplevelRequest;
use crate::wayland_subscription::ToplevelUpdate;
use crate::wayland_subscription::WaylandRequest;
use crate::wayland_subscription::WaylandUpdate;
use cctk::sctk::reexports::calloop::channel::Sender;
use cctk::toplevel_info::ToplevelInfo;
use cctk::wayland_client::protocol::wl_data_device_manager::DndAction;
use cctk::wayland_client::protocol::wl_seat::WlSeat;
use cosmic::cosmic_config::{Config, CosmicConfigEntry};
use cosmic::desktop::{load_applications_for_app_ids, DesktopEntryData};
use cosmic::iced;
use cosmic::iced::event::listen_with;
use cosmic::iced::wayland::actions::data_device::DataFromMimeType;
use cosmic::iced::wayland::actions::data_device::DndIcon;
use cosmic::iced::wayland::popup::destroy_popup;
use cosmic::iced::wayland::popup::get_popup;
use cosmic::iced::widget::dnd_listener;
use cosmic::iced::widget::vertical_rule;
use cosmic::iced::widget::vertical_space;
use cosmic::iced::widget::{column, dnd_source, mouse_area, row, Column, Row};
use cosmic::iced::Color;
use cosmic::iced::{window, Subscription};
use cosmic::iced_core::Border;
use cosmic::iced_core::Padding;
use cosmic::iced_core::Shadow;
use cosmic::iced_runtime::core::alignment::Horizontal;
use cosmic::iced_runtime::core::event;
use cosmic::iced_sctk::commands::data_device::accept_mime_type;
use cosmic::iced_sctk::commands::data_device::finish_dnd;
use cosmic::iced_sctk::commands::data_device::request_dnd_data;
use cosmic::iced_sctk::commands::data_device::set_actions;
use cosmic::iced_sctk::commands::data_device::start_drag;
use cosmic::iced_style::application;
use cosmic::theme::Button;
use cosmic::widget::divider;
use cosmic::widget::rectangle_tracker::rectangle_tracker_subscription;
use cosmic::widget::rectangle_tracker::RectangleTracker;
use cosmic::widget::rectangle_tracker::RectangleUpdate;
use cosmic::{
    applet::{cosmic_panel_config::PanelAnchor, Context},
    Command,
};
use cosmic::{Element, Theme};
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1;
use futures::future::pending;
use iced::widget::container;
use iced::Alignment;
use iced::Background;
use iced::Length;
use itertools::Itertools;
use rand::{thread_rng, Rng};
use std::cmp::min;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use switcheroo_control::Gpu;
use tokio::time::sleep;
use url::Url;

static MIME_TYPE: &str = "text/uri-list";

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CosmicAppList>(true, ())
}

pub fn load_applications_for_app_ids_sorted<'a, 'b>(
    locale: impl Into<Option<&'a str>>,
    app_ids: impl Iterator<Item = &'b str> + Clone,
    fill_missing_ones: bool,
) -> Vec<DesktopEntryData> {
    let mut ret = load_applications_for_app_ids(locale, app_ids.clone(), fill_missing_ones);
    ret.sort_by(|a, b| {
        app_ids
            .clone()
            .position(|id| id == a.id)
            .unwrap()
            .cmp(&app_ids.clone().position(|id| id == b.id).unwrap())
    });

    ret
}

#[derive(Debug, Clone, Default)]
struct DockItem {
    id: u32,
    toplevels: Vec<(ZcosmicToplevelHandleV1, ToplevelInfo)>,
    desktop_info: DesktopEntryData,
}

impl DataFromMimeType for DockItem {
    fn from_mime_type(&self, mime_type: &str) -> Option<Vec<u8>> {
        if mime_type == MIME_TYPE && self.desktop_info.path.is_some() {
            Some(
                Url::from_file_path(self.desktop_info.path.as_deref().unwrap())
                    .ok()?
                    .to_string()
                    .as_bytes()
                    .to_vec(),
            )
        } else {
            None
        }
    }
}

impl DockItem {
    fn new(
        id: u32,
        toplevels: Vec<(ZcosmicToplevelHandleV1, ToplevelInfo)>,
        desktop_info: DesktopEntryData,
    ) -> Self {
        Self {
            id,
            toplevels,
            desktop_info,
        }
    }

    fn as_icon(
        &self,
        applet: &Context,
        rectangle_tracker: Option<&RectangleTracker<u32>>,
        interaction_enabled: bool,
        gpus: Option<&[Gpu]>,
    ) -> Element<'_, Message> {
        let Self {
            toplevels,
            desktop_info,
            id,
            ..
        } = self;

        let cosmic_icon = desktop_info
            .icon
            .as_cosmic_icon()
            .size(applet.suggested_size().0 + 6);

        let dot_radius = 2;
        let dot_spacer = (0..1)
            .map(|_| {
                container(vertical_space(Length::Fixed(0.0)))
                    .padding(dot_radius)
                    .into()
            })
            .collect_vec();

        let dots = if toplevels.is_empty() {
            (0..1)
                .map(|_| {
                    container(vertical_space(Length::Fixed(0.0)))
                        .padding(dot_radius)
                        .into()
                })
                .collect_vec()
        } else {
            (0..min(toplevels.len(), 3))
                .map(|_| {
                    container(vertical_space(Length::Fixed(0.0)))
                        .padding(dot_radius)
                        .style(<Theme as container::StyleSheet>::Style::Custom(Box::new(
                            |theme| container::Appearance {
                                text_color: Some(Color::TRANSPARENT),
                                background: Some(Background::Color(
                                    theme.cosmic().on_bg_color().into(),
                                )),
                                border: Border {
                                    radius: 4.0.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                shadow: Shadow::default(),
                                icon_color: Some(Color::TRANSPARENT),
                            },
                        )))
                        .into()
                })
                .collect_vec()
        };

        let icon_wrapper: Element<_> = match applet.anchor {
            PanelAnchor::Left => row(vec![
                column(dots).spacing(4).into(),
                cosmic_icon.into(),
                column(dot_spacer).spacing(4).into(),
            ])
            .align_items(iced::Alignment::Center)
            .spacing(1)
            .into(),
            PanelAnchor::Right => row(vec![
                column(dot_spacer).spacing(4).into(),
                cosmic_icon.into(),
                column(dots).spacing(4).into(),
            ])
            .align_items(iced::Alignment::Center)
            .spacing(1)
            .into(),
            PanelAnchor::Top => column(vec![
                row(dots).spacing(4).into(),
                cosmic_icon.into(),
                row(dot_spacer).spacing(4).into(),
            ])
            .align_items(iced::Alignment::Center)
            .spacing(1)
            .into(),
            PanelAnchor::Bottom => column(vec![
                row(dot_spacer).spacing(4).into(),
                cosmic_icon.into(),
                row(dots).spacing(4).into(),
            ])
            .align_items(iced::Alignment::Center)
            .spacing(1)
            .into(),
        };

        let icon_button = match applet.anchor {
            PanelAnchor::Left => cosmic::widget::button(icon_wrapper)
                .style(Button::Text)
                .padding([5, 0]),
            PanelAnchor::Right => cosmic::widget::button(icon_wrapper)
                .style(Button::Text)
                .padding([5, 0]),
            PanelAnchor::Top => cosmic::widget::button(icon_wrapper)
                .style(Button::Text)
                .padding([0, 5]),
            PanelAnchor::Bottom => cosmic::widget::button(icon_wrapper)
                .style(Button::Text)
                .padding([0, 5]),
        };

        let icon_button = if interaction_enabled {
            dnd_source(
                mouse_area(
                    icon_button
                        .on_press_maybe(
                            toplevels
                                .first()
                                .map(|t| Message::Activate(t.0.clone()))
                                .or_else(|| {
                                    let gpu_idx = gpus.map(|gpus| {
                                        if desktop_info.prefers_dgpu {
                                            gpus.iter().position(|gpu| !gpu.default).unwrap_or(0)
                                        } else {
                                            gpus.iter().position(|gpu| gpu.default).unwrap_or(0)
                                        }
                                    });

                                    desktop_info
                                        .exec
                                        .clone()
                                        .map(|exec| Message::Exec(exec, gpu_idx))
                                }),
                        )
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                )
                .on_right_release(Message::Popup(desktop_info.id.clone())),
            )
            .on_drag(|_| Message::StartDrag(desktop_info.id.clone()))
            .on_cancelled(Message::DragFinished)
            .on_finished(Message::DragFinished)
        } else {
            dnd_source(icon_button)
        };

        if let Some(tracker) = rectangle_tracker {
            tracker.container(*id, icon_button).into()
        } else {
            icon_button.into()
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DndOffer {
    dock_item: Option<DockItem>,
    preview_index: usize,
}

#[derive(Clone, Default)]
struct CosmicAppList {
    core: cosmic::app::Core,
    popup: Option<(window::Id, u32)>,
    subscription_ctr: u32,
    item_ctr: u32,
    active_list: Vec<DockItem>,
    favorite_list: Vec<DockItem>,
    dnd_source: Option<(window::Id, DockItem, DndAction)>,
    config: AppListConfig,
    wayland_sender: Option<Sender<WaylandRequest>>,
    seat: Option<WlSeat>,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangles: HashMap<u32, iced::Rectangle>,
    dnd_offer: Option<DndOffer>,
    is_listening_for_dnd: bool,
    gpus: Option<Vec<Gpu>>,
}

// TODO DnD after sctk merges DnD
#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandUpdate),
    Favorite(String),
    UnFavorite(String),
    Popup(String),
    GpuRequest(Option<Vec<Gpu>>),
    CloseRequested(window::Id),
    ClosePopup,
    Activate(ZcosmicToplevelHandleV1),
    Exec(String, Option<usize>),
    Quit(String),
    Ignore,
    NewSeat(WlSeat),
    RemovedSeat(WlSeat),
    Rectangle(RectangleUpdate<u32>),
    StartDrag(String), // id of the DockItem
    DragFinished,
    DndEnter(f32, f32),
    DndExit,
    DndMotion(f32, f32),
    DndDrop,
    DndData(PathBuf),
    StartListeningForDnd,
    StopListeningForDnd,
    IncrementSubscriptionCtr,
    ConfigUpdated(AppListConfig),
}

fn index_in_list(
    mut list_len: usize,
    item_size: f32,
    divider_size: f32,
    existing_preview: Option<usize>,
    pos_in_list: f32,
) -> usize {
    if existing_preview.is_some() {
        list_len += 1;
    }
    let total_len = list_len as f32 * (item_size + divider_size) - divider_size;
    let pos_in_list = pos_in_list * total_len;
    let index = if (list_len == 0) || (pos_in_list < item_size / 2.0) {
        0
    } else {
        let mut i = 1;
        let mut pos = item_size / 2.0;
        while i < list_len {
            let next_pos = pos + item_size + divider_size;
            if pos > pos_in_list && pos_in_list < next_pos {
                break;
            }
            pos = next_pos;
            i += 1;
        }
        i
    };

    if let Some(existing_preview) = existing_preview {
        if index >= existing_preview {
            index.checked_sub(1).unwrap_or_default()
        } else {
            index
        }
    } else {
        index
    }
}

async fn try_get_gpus() -> Option<Vec<Gpu>> {
    let connection = zbus::Connection::system().await.ok()?;
    let proxy = switcheroo_control::SwitcherooControlProxy::new(&connection)
        .await
        .ok()?;

    if !proxy.has_dual_gpu().await.ok()? {
        return None;
    }

    let gpus = proxy.get_gpus().await.ok()?;
    if gpus.is_empty() {
        return None;
    }

    Some(gpus)
}

pub fn menu_button<'a, Message>(
    content: impl Into<Element<'a, Message>>,
) -> cosmic::widget::Button<'a, Message, cosmic::Theme, cosmic::Renderer> {
    cosmic::widget::Button::new(content)
        .style(Button::AppletMenu)
        .padding(menu_control_padding())
        .width(Length::Fill)
}

pub fn menu_control_padding() -> Padding {
    let theme = cosmic::theme::active();
    let cosmic = theme.cosmic();
    [cosmic.space_xxs(), cosmic.space_m()].into()
}

impl cosmic::Application for CosmicAppList {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, iced::Command<cosmic::app::Message<Self::Message>>) {
        let config = Config::new(APP_ID, AppListConfig::VERSION)
            .ok()
            .and_then(|c| AppListConfig::get_entry(&c).ok())
            .unwrap_or_default();
        let mut self_ = Self {
            core,
            favorite_list: load_applications_for_app_ids_sorted(
                None,
                config.favorites.iter().map(|s| &**s),
                true,
            )
            .into_iter()
            .enumerate()
            .map(|(favorite_ctr, e)| DockItem {
                id: favorite_ctr as u32,
                toplevels: Default::default(),
                desktop_info: e,
            })
            .collect(),
            config,
            ..Default::default()
        };
        self_.item_ctr = self_.favorite_list.len() as u32;

        (
            self_,
            Command::perform(try_get_gpus(), |gpus| {
                cosmic::app::Message::App(Message::GpuRequest(gpus))
            }),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn update(
        &mut self,
        message: Self::Message,
    ) -> iced::Command<cosmic::app::Message<Self::Message>> {
        match message {
            Message::Popup(id) => {
                if let Some((popup_id, _toplevel)) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
                if let Some(toplevel_group) = self
                    .active_list
                    .iter()
                    .chain(self.favorite_list.iter())
                    .find(|t| t.desktop_info.id == id)
                {
                    let rectangle = match self.rectangles.get(&toplevel_group.id) {
                        Some(r) => r,
                        None => return Command::none(),
                    };

                    let new_id = window::Id::unique();
                    self.popup = Some((new_id, toplevel_group.id));

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        window::Id::MAIN,
                        new_id,
                        None,
                        None,
                        None,
                    );
                    let iced::Rectangle {
                        x,
                        y,
                        width,
                        height,
                    } = *rectangle;
                    popup_settings.positioner.anchor_rect = iced::Rectangle::<i32> {
                        x: x as i32,
                        y: y as i32,
                        width: width as i32,
                        height: height as i32,
                    };

                    let gpu_update = Command::perform(try_get_gpus(), |gpus| {
                        cosmic::app::Message::App(Message::GpuRequest(gpus))
                    });
                    return Command::batch([gpu_update, get_popup(popup_settings)]);
                }
            }
            Message::Favorite(id) => {
                if let Some(i) = self
                    .active_list
                    .iter()
                    .position(|t| t.desktop_info.id == id)
                {
                    let entry = self.active_list.remove(i);
                    self.favorite_list.push(entry);
                }

                self.config
                    .add_favorite(id, &Config::new(APP_ID, AppListConfig::VERSION).unwrap());
                if let Some((popup_id, _toplevel)) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::UnFavorite(id) => {
                self.config.remove_favorite(
                    id.clone(),
                    &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                );
                if let Some(i) = self
                    .favorite_list
                    .iter()
                    .position(|t| t.desktop_info.id == id)
                {
                    let entry = self.favorite_list.remove(i);
                    self.rectangles.remove(&entry.id);
                    if !entry.toplevels.is_empty() {
                        self.active_list.push(entry);
                    }
                }
                if let Some((popup_id, _toplevel)) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::Activate(handle) => {
                if let Some(tx) = self.wayland_sender.as_ref() {
                    let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Activate(handle)));
                }
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.0);
                }
            }
            Message::Quit(id) => {
                if let Some(toplevel_group) = self
                    .active_list
                    .iter()
                    .chain(self.favorite_list.iter())
                    .find(|t| t.desktop_info.id == id)
                {
                    for (handle, _) in &toplevel_group.toplevels {
                        if let Some(tx) = self.wayland_sender.as_ref() {
                            let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Quit(
                                handle.clone(),
                            )));
                        }
                    }
                }
                if let Some((popup_id, _toplevel)) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::StartDrag(id) => {
                if let Some((is_favorite, toplevel_group)) = self
                    .active_list
                    .iter()
                    .find_map(|t| {
                        if t.desktop_info.id == id {
                            Some((false, t.clone()))
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        if let Some(pos) = self
                            .favorite_list
                            .iter()
                            .position(|t| t.desktop_info.id == id)
                        {
                            let t = self.favorite_list.remove(pos);
                            self.config.remove_favorite(
                                t.desktop_info.id.clone(),
                                &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                            );
                            Some((true, t))
                        } else {
                            None
                        }
                    })
                {
                    let icon_id = window::Id::unique();
                    self.dnd_source = Some((icon_id, toplevel_group.clone(), DndAction::empty()));
                    return start_drag(
                        vec![MIME_TYPE.to_string()],
                        if is_favorite {
                            DndAction::all()
                        } else {
                            DndAction::Copy
                        },
                        window::Id::MAIN,
                        Some(DndIcon::Custom(icon_id)),
                        Box::new(toplevel_group),
                    );
                }
            }
            Message::DragFinished => {
                if let Some((_, mut toplevel_group, _)) = self.dnd_source.take() {
                    if !self
                        .favorite_list
                        .iter()
                        .chain(self.active_list.iter())
                        .any(|t| t.desktop_info.id == toplevel_group.desktop_info.id)
                        && !toplevel_group.toplevels.is_empty()
                    {
                        self.item_ctr += 1;
                        toplevel_group.id = self.item_ctr;
                        self.active_list.push(toplevel_group);
                    }
                }
            }
            Message::DndEnter(x, y) => {
                let item_size = self.core.applet.suggested_size().0;
                let pos_in_list = match self.core.applet.anchor {
                    PanelAnchor::Top | PanelAnchor::Bottom => x,
                    PanelAnchor::Left | PanelAnchor::Right => y,
                };
                let num_favs = self.favorite_list.len();
                let index = index_in_list(num_favs, item_size as f32, 4.0, None, pos_in_list);
                self.dnd_offer = Some(DndOffer {
                    preview_index: index,
                    ..DndOffer::default()
                });
                let mut cmds = vec![
                    accept_mime_type(Some(MIME_TYPE.to_string())),
                    set_actions(
                        if self.dnd_source.is_some() {
                            DndAction::Move
                        } else {
                            DndAction::Copy
                        },
                        DndAction::all(),
                    ),
                ];
                if let Some(dnd_source) = self.dnd_source.as_ref() {
                    self.dnd_offer.as_mut().unwrap().dock_item = Some(dnd_source.1.clone());
                } else {
                    cmds.push(request_dnd_data(MIME_TYPE.to_string()));
                }
                return Command::batch(cmds);
            }
            Message::DndMotion(x, y) => {
                if let Some(DndOffer { preview_index, .. }) = self.dnd_offer.as_mut() {
                    let item_size = self.core.applet.suggested_size().0;
                    let pos_in_list = match self.core.applet.anchor {
                        PanelAnchor::Top | PanelAnchor::Bottom => x,
                        PanelAnchor::Left | PanelAnchor::Right => y,
                    };
                    let num_favs = self.favorite_list.len();
                    let index = index_in_list(
                        num_favs,
                        item_size as f32,
                        4.0,
                        Some(*preview_index),
                        pos_in_list,
                    );
                    *preview_index = index;
                }
            }
            Message::DndExit => {
                self.dnd_offer = None;
                return accept_mime_type(None);
            }
            Message::DndData(file_path) => {
                if let Some(DndOffer { dock_item, .. }) = self.dnd_offer.as_mut() {
                    if let Some(di) = cosmic::desktop::load_desktop_file(None, file_path) {
                        self.item_ctr += 1;
                        *dock_item = Some(DockItem::new(self.item_ctr, Vec::new(), di));
                    }
                }
            }
            Message::DndDrop => {
                // we actually should have the data already, if not, we probably shouldn't do
                // anything anyway
                if let Some((mut dock_item, index)) = self
                    .dnd_offer
                    .take()
                    .and_then(|o| o.dock_item.map(|i| (i, o.preview_index)))
                {
                    self.item_ctr += 1;

                    if let Some((pos, is_favorite)) = self
                        .active_list
                        .iter()
                        .position(|DockItem { desktop_info, .. }| {
                            desktop_info.id == dock_item.desktop_info.id
                        })
                        .map(|pos| (pos, false))
                        .or_else(|| {
                            self.favorite_list
                                .iter()
                                .position(|DockItem { desktop_info, .. }| {
                                    desktop_info.id == dock_item.desktop_info.id
                                })
                                .map(|pos| (pos, true))
                        })
                    {
                        let t = if is_favorite {
                            self.favorite_list.remove(pos)
                        } else {
                            self.active_list.remove(pos)
                        };
                        dock_item.toplevels = t.toplevels;
                    };
                    dock_item.id = self.item_ctr;

                    self.favorite_list
                        .insert(index.min(self.favorite_list.len()), dock_item);
                    self.config.update_favorites(
                        self.favorite_list
                            .iter()
                            .map(|dock_item| dock_item.desktop_info.id.clone())
                            .collect(),
                        &Config::new(APP_ID, AppListConfig::VERSION).unwrap(),
                    );
                }
                return finish_dnd();
            }
            Message::Wayland(event) => {
                match event {
                    WaylandUpdate::Init(tx) => {
                        self.wayland_sender.replace(tx);
                    }
                    WaylandUpdate::Finished => {
                        for t in &mut self.favorite_list {
                            t.toplevels.clear();
                        }
                        self.active_list.clear();
                        let subscription_ctr = self.subscription_ctr;
                        let mut rng = thread_rng();
                        let rand_d = rng.gen_range(0..100);
                        return iced::Command::perform(
                            async move {
                                if let Some(millis) = 2u64
                                    .checked_pow(subscription_ctr)
                                    .and_then(|d| d.checked_add(rand_d))
                                {
                                    sleep(Duration::from_millis(millis)).await;
                                } else {
                                    pending::<()>().await;
                                }
                            },
                            |_| Message::IncrementSubscriptionCtr,
                        )
                        .map(cosmic::app::message::app);
                    }
                    WaylandUpdate::Toplevel(event) => match event {
                        ToplevelUpdate::Add(handle, mut info) => {
                            if let Some(t) = self
                                .active_list
                                .iter_mut()
                                .chain(self.favorite_list.iter_mut())
                                .find(|DockItem { desktop_info, .. }| {
                                    desktop_info.id == info.app_id
                                        || desktop_info.wm_class.as_ref() == Some(&info.app_id)
                                })
                            {
                                t.toplevels.push((handle, info));
                            } else {
                                if info.app_id.is_empty() {
                                    info.app_id = format!("Unknown Application {}", self.item_ctr);
                                }
                                self.item_ctr += 1;
                                let desktop_info = load_applications_for_app_ids_sorted(
                                    None,
                                    std::iter::once(&*info.app_id),
                                    true,
                                )
                                .into_iter()
                                .next()
                                .unwrap();
                                self.active_list.push(DockItem {
                                    id: self.item_ctr,
                                    toplevels: vec![(handle, info)],
                                    desktop_info,
                                });
                            }
                        }
                        ToplevelUpdate::Remove(handle) => {
                            for t in self
                                .active_list
                                .iter_mut()
                                .chain(self.favorite_list.iter_mut())
                            {
                                t.toplevels.retain(|(t_handle, _)| t_handle != &handle);
                            }
                            self.active_list.retain(|t| !t.toplevels.is_empty());
                        }
                        ToplevelUpdate::Update(handle, info) => {
                            // TODO probably want to make sure it is removed
                            if info.app_id.is_empty() {
                                return Command::none();
                            }
                            'toplevel_loop: for toplevel_list in self
                                .active_list
                                .iter_mut()
                                .chain(self.favorite_list.iter_mut())
                            {
                                for (t_handle, t_info) in &mut toplevel_list.toplevels {
                                    if &handle == t_handle {
                                        *t_info = info;
                                        break 'toplevel_loop;
                                    }
                                }
                            }
                        }
                    },
                    WaylandUpdate::ActivationToken {
                        token,
                        exec,
                        gpu_idx,
                    } => {
                        let mut envs = Vec::new();
                        if let Some(token) = token {
                            envs.push(("XDG_ACTIVATION_TOKEN".to_string(), token.clone()));
                            envs.push(("DESKTOP_STARTUP_ID".to_string(), token));
                        }
                        if let (Some(gpus), Some(idx)) = (self.gpus.as_ref(), gpu_idx) {
                            envs.extend(
                                gpus[idx]
                                    .environment
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.clone())),
                            );
                        }
                        tokio::task::spawn_blocking(|| {
                            cosmic::desktop::spawn_desktop_exec(exec, envs);
                        });
                    }
                }
            }
            Message::NewSeat(s) => {
                self.seat.replace(s);
            }
            Message::RemovedSeat(_) => {
                self.seat.take();
            }
            Message::Exec(exec, gpu_idx) => {
                if let Some(tx) = self.wayland_sender.as_ref() {
                    let _ = tx.send(WaylandRequest::TokenRequest {
                        app_id: Self::APP_ID.to_string(),
                        exec,
                        gpu_idx,
                    });
                }
            }
            Message::Rectangle(u) => match u {
                RectangleUpdate::Rectangle(r) => {
                    self.rectangles.insert(r.0, r.1);
                }
                RectangleUpdate::Init(tracker) => {
                    self.rectangle_tracker.replace(tracker);
                }
            },
            Message::Ignore => {}
            Message::ClosePopup => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.0);
                }
            }
            Message::StartListeningForDnd => {
                self.is_listening_for_dnd = true;
            }
            Message::StopListeningForDnd => {
                self.is_listening_for_dnd = false;
            }
            Message::IncrementSubscriptionCtr => {
                self.subscription_ctr += 1;
            }
            Message::ConfigUpdated(config) => {
                self.config = config;
                // drain to active list
                for item in self.favorite_list.drain(..) {
                    self.active_list.push(item);
                }

                // pull back configured items into the favorites list
                self.favorite_list = load_applications_for_app_ids_sorted(
                    None,
                    self.config.favorites.iter().map(|s| &**s),
                    true,
                )
                .into_iter()
                .map(|new_dock_item| {
                    if let Some(p) = self
                        .active_list
                        .iter()
                        .position(|dock_item| dock_item.desktop_info.id == new_dock_item.id)
                    {
                        self.active_list.remove(p)
                    } else {
                        self.item_ctr += 1;
                        DockItem {
                            id: self.item_ctr,
                            toplevels: Default::default(),
                            desktop_info: new_dock_item,
                        }
                    }
                })
                .collect();
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup.as_ref().map(|p| p.0) {
                    self.popup = None;
                }
            }
            Message::GpuRequest(gpus) => {
                self.gpus = gpus;
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<Message> {
        let is_horizontal = match self.core.applet.anchor {
            PanelAnchor::Top | PanelAnchor::Bottom => true,
            PanelAnchor::Left | PanelAnchor::Right => false,
        };
        let mut favorites: Vec<_> = self
            .favorite_list
            .iter()
            .map(|dock_item| {
                dock_item.as_icon(
                    &self.core.applet,
                    self.rectangle_tracker.as_ref(),
                    self.popup.is_none(),
                    self.gpus.as_deref(),
                )
            })
            .collect();

        if let Some((item, index)) = self
            .dnd_offer
            .as_ref()
            .and_then(|o| o.dock_item.as_ref().map(|item| (item, o.preview_index)))
        {
            favorites.insert(
                index,
                item.as_icon(&self.core.applet, None, false, self.gpus.as_deref()),
            );
        } else if self.is_listening_for_dnd && self.favorite_list.is_empty() {
            // show star indicating favorite_list is drag target
            favorites.push(
                container(
                    cosmic::widget::icon::from_name("starred-symbolic.symbolic")
                        .size(self.core.applet.suggested_size().0),
                )
                .padding(8)
                .into(),
            );
        }

        let active: Vec<_> = self
            .active_list
            .iter()
            .map(|dock_item| {
                dock_item.as_icon(
                    &self.core.applet,
                    self.rectangle_tracker.as_ref(),
                    self.popup.is_none(),
                    self.gpus.as_deref(),
                )
            })
            .collect();

        let (w, h, favorites, active, divider) = if is_horizontal {
            (
                Length::Shrink,
                Length::Shrink,
                dnd_listener(row(favorites)),
                row(active).into(),
                container(vertical_rule(1))
                    .height(self.core.applet.suggested_size().1)
                    .into(),
            )
        } else {
            (
                Length::Shrink,
                Length::Shrink,
                dnd_listener(column(favorites)),
                column(active).into(),
                container(divider::horizontal::default())
                    .width(self.core.applet.suggested_size().1)
                    .into(),
            )
        };

        let favorites = favorites
            .on_enter(|_actions, mime_types, location| {
                if self.is_listening_for_dnd || mime_types.iter().any(|m| m == MIME_TYPE) {
                    Message::DndEnter(location.0, location.1)
                } else {
                    Message::Ignore
                }
            })
            .on_motion(if self.dnd_offer.is_some() {
                Message::DndMotion
            } else {
                |_, _| Message::Ignore
            })
            .on_exit(Message::DndExit)
            .on_drop(Message::DndDrop)
            .on_data(|mime_type, data| {
                if mime_type == MIME_TYPE {
                    if let Some(p) = String::from_utf8(data)
                        .ok()
                        .and_then(|s| Url::from_str(&s).ok())
                        .and_then(|u| u.to_file_path().ok())
                    {
                        Message::DndData(p)
                    } else {
                        Message::Ignore
                    }
                } else {
                    Message::Ignore
                }
            });

        let show_favorites =
            !self.favorite_list.is_empty() || self.dnd_offer.is_some() || self.is_listening_for_dnd;
        let content_list: Vec<Element<_>> = if show_favorites && !self.active_list.is_empty() {
            vec![favorites.into(), divider, active]
        } else if show_favorites {
            vec![favorites.into()]
        } else if !self.active_list.is_empty() {
            vec![active]
        } else {
            vec![
                cosmic::widget::icon::from_name("com.system76.CosmicAppList")
                    .size(self.core.applet.suggested_size().0)
                    .into(),
            ]
        };

        let mut content = match &self.core.applet.anchor {
            PanelAnchor::Left | PanelAnchor::Right => container(
                Column::with_children(content_list)
                    .spacing(4)
                    .align_items(Alignment::Center)
                    .height(h)
                    .width(w),
            ),
            PanelAnchor::Top | PanelAnchor::Bottom => container(
                Row::with_children(content_list)
                    .spacing(4)
                    .align_items(Alignment::Center)
                    .height(h)
                    .width(w),
            ),
        };
        if self.active_list.is_empty() && self.favorite_list.is_empty() {
            let suggested_size = self.core.applet.suggested_size();
            content = content.width(suggested_size.0).height(suggested_size.1);
        }
        if self.popup.is_some() {
            mouse_area(content)
                .on_right_release(Message::ClosePopup)
                .on_press(Message::ClosePopup)
                .into()
        } else {
            content.into()
        }
    }

    fn view_window(&self, id: window::Id) -> Element<Message> {
        if let Some((_, item, _)) = self.dnd_source.as_ref().filter(|s| s.0 == id) {
            item.desktop_info
                .icon
                .as_cosmic_icon()
                .size(self.core.applet.suggested_size().0)
                .into()
        } else if let Some((_popup_id, id)) = self.popup.as_ref().filter(|p| id == p.0) {
            let Some(DockItem {
                toplevels,
                desktop_info,
                ..
            }) = self
                .favorite_list
                .iter()
                .chain(self.active_list.iter())
                .find(|i| i.id == *id)
            else {
                return iced::widget::text("").into();
            };

            let is_favorite = self.config.favorites.contains(&desktop_info.id)
                || desktop_info
                    .wm_class
                    .as_ref()
                    .is_some_and(|wm_class| self.config.favorites.contains(wm_class));

            let mut content = column![container(
                iced::widget::text(&desktop_info.name).horizontal_alignment(Horizontal::Center)
            )
            .padding(menu_control_padding()),]
            .padding([8, 0])
            .align_items(Alignment::Center);

            if let Some(exec) = desktop_info.exec.clone() {
                if !toplevels.is_empty() {
                    content = content.push(
                        menu_button(iced::widget::text(fl!("new-window")))
                            .on_press(Message::Exec(exec, None)),
                    );
                } else if let Some(gpus) = self.gpus.as_ref() {
                    let default_idx = if desktop_info.prefers_dgpu {
                        gpus.iter().position(|gpu| !gpu.default).unwrap_or(0)
                    } else {
                        gpus.iter().position(|gpu| gpu.default).unwrap_or(0)
                    };
                    for (i, gpu) in gpus.iter().enumerate() {
                        content = content.push(
                            menu_button(iced::widget::text(format!(
                                "{} {}",
                                fl!("run-on", gpu = gpu.name.clone()),
                                if i == default_idx {
                                    fl!("run-on-default")
                                } else {
                                    String::new()
                                }
                            )))
                            .on_press(Message::Exec(exec.clone(), Some(i))),
                        );
                    }
                } else {
                    content = content.push(
                        menu_button(iced::widget::text(fl!("run")))
                            .on_press(Message::Exec(exec, None)),
                    );
                }
                content = content.push(divider::horizontal::default());
            }

            if !toplevels.is_empty() {
                let mut list_col = column![];
                for (handle, info) in toplevels {
                    let title = if info.title.len() > 20 {
                        format!("{:.24}...", &info.title)
                    } else {
                        info.title.clone()
                    };
                    list_col = list_col.push(
                        menu_button(iced::widget::text(title))
                            .on_press(Message::Activate(handle.clone())),
                    );
                }
                content = content.push(list_col);
                content = content.push(divider::horizontal::default());
            }
            content = content.push(if is_favorite {
                menu_button(iced::widget::text(fl!("unfavorite")))
                    .on_press(Message::UnFavorite(desktop_info.id.clone()))
            } else {
                menu_button(iced::widget::text(fl!("favorite")))
                    .on_press(Message::Favorite(desktop_info.id.clone()))
            });

            content = match toplevels.len() {
                0 => content,
                1 => content.push(
                    menu_button(iced::widget::text(fl!("quit")))
                        .on_press(Message::Quit(desktop_info.id.clone())),
                ),
                _ => content.push(
                    menu_button(iced::widget::text(&fl!("quit-all")))
                        .on_press(Message::Quit(desktop_info.id.clone())),
                ),
            };
            self.core.applet.popup_container(content).into()
        } else {
            let suggested = self.core.applet.suggested_size();
            iced::widget::row!()
                .width(Length::Fixed(suggested.0 as f32))
                .height(Length::Fixed(suggested.1 as f32))
                .into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            wayland_subscription().map(Message::Wayland),
            listen_with(|e, _| match e {
                cosmic::iced_runtime::core::Event::PlatformSpecific(
                    event::PlatformSpecific::Wayland(event::wayland::Event::Seat(e, seat)),
                ) => match e {
                    event::wayland::SeatEvent::Enter => Some(Message::NewSeat(seat)),
                    event::wayland::SeatEvent::Leave => Some(Message::RemovedSeat(seat)),
                },
                // XXX Must be done to catch a finished drag after the source is removed
                // (for now, the source is removed when the drag starts)
                cosmic::iced_runtime::core::Event::PlatformSpecific(
                    event::PlatformSpecific::Wayland(event::wayland::Event::DataSource(
                        event::wayland::DataSourceEvent::DndFinished
                        | event::wayland::DataSourceEvent::Cancelled,
                    )),
                ) => Some(Message::DragFinished),
                cosmic::iced_runtime::core::Event::PlatformSpecific(
                    event::PlatformSpecific::Wayland(event::wayland::Event::DndOffer(
                        event::wayland::DndOfferEvent::Enter { mime_types, .. },
                    )),
                ) => {
                    if mime_types.iter().any(|m| m == MIME_TYPE) {
                        Some(Message::StartListeningForDnd)
                    } else {
                        None
                    }
                }
                cosmic::iced_runtime::core::Event::PlatformSpecific(
                    event::PlatformSpecific::Wayland(event::wayland::Event::DndOffer(
                        event::wayland::DndOfferEvent::Leave
                        | event::wayland::DndOfferEvent::DropPerformed,
                    )),
                ) => Some(Message::StopListeningForDnd),
                _ => None,
            }),
            rectangle_tracker_subscription(0).map(|update| Message::Rectangle(update.1)),
            self.core.watch_config(APP_ID).map(|u| {
                for err in u.errors {
                    log::error!("Error watching config: {}", err);
                }
                Message::ConfigUpdated(u.config)
            }),
        ])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}
