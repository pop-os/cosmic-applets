use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use crate::config;
use crate::config::AppListConfig;
use crate::fl;
use crate::toplevel_subscription::toplevel_subscription;
use crate::toplevel_subscription::ToplevelRequest;
use crate::toplevel_subscription::ToplevelUpdate;
use calloop::channel::Sender;
use cctk::toplevel_info::ToplevelInfo;
use cctk::wayland_client::protocol::wl_data_device_manager::DndAction;
use cctk::wayland_client::protocol::wl_seat::WlSeat;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::applet::CosmicAppletHelper;
use cosmic::iced;
use cosmic::iced::wayland::actions::data_device::DataFromMimeType;
use cosmic::iced::wayland::actions::data_device::DndIcon;
use cosmic::iced::wayland::actions::window::SctkWindowSettings;
use cosmic::iced::wayland::popup::destroy_popup;
use cosmic::iced::wayland::popup::get_popup;
use cosmic::iced::widget::{column, dnd_source, mouse_listener, row, text, Column, Row};
use cosmic::iced::Settings;
use cosmic::iced::{window, Application, Command, Subscription};
use cosmic::iced_native as native;
use cosmic::iced_native::alignment::Horizontal;
use cosmic::iced_native::subscription::events_with;
use cosmic::iced_native::widget::vertical_space;
use cosmic::iced_sctk::commands::data_device::accept_mime_type;
use cosmic::iced_sctk::commands::data_device::finish_dnd;
use cosmic::iced_sctk::commands::data_device::request_dnd_data;
use cosmic::iced_sctk::commands::data_device::set_actions;
use cosmic::iced_sctk::commands::data_device::start_drag;
use cosmic::iced_sctk::layout::Limits;
use cosmic::iced_sctk::settings::InitialSurface;
use cosmic::iced_sctk::widget::dnd_listener;
use cosmic::iced_sctk::widget::vertical_rule;
use cosmic::iced_style::application::{self, Appearance};
use cosmic::iced_style::Color;
use cosmic::theme::Button;
use cosmic::widget::divider;
use cosmic::widget::rectangle_tracker::rectangle_tracker_subscription;
use cosmic::widget::rectangle_tracker::RectangleTracker;
use cosmic::widget::rectangle_tracker::RectangleUpdate;
use cosmic::{Element, Theme};
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1;
use freedesktop_desktop_entry::DesktopEntry;
use iced::widget::container;
use iced::Alignment;
use iced::Background;
use iced::Length;
use itertools::Itertools;
use native::event;
use url::Url;

static MIME_TYPE: &str = "text/uri-list";

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    let pixel_size = helper.suggested_size().0;
    let padding = 8;
    let dot_size = 4;
    let spacing = 4;
    let thickness = (pixel_size + 2 * padding + dot_size + spacing) as u32;
    let (w, h) = match helper.anchor {
        PanelAnchor::Top | PanelAnchor::Bottom => (2000, thickness),
        PanelAnchor::Left | PanelAnchor::Right => (thickness, 2000),
    };

    CosmicAppList::run(Settings {
        initial_surface: InitialSurface::XdgWindow(SctkWindowSettings {
            iced_settings: cosmic::iced_native::window::Settings {
                ..Default::default()
            },
            autosize: true,
            size_limits: Limits::NONE
                .min_height(1)
                .min_width(1)
                .max_height(h)
                .max_width(w),
            ..Default::default()
        }),
        ..Default::default()
    })
}

#[derive(Debug, Clone, Default)]
struct DockItem {
    id: u32,
    toplevels: Vec<(ZcosmicToplevelHandleV1, ToplevelInfo)>,
    desktop_info: DesktopInfo,
}

impl DataFromMimeType for DockItem {
    fn from_mime_type(&self, mime_type: &str) -> Option<Vec<u8>> {
        if mime_type == MIME_TYPE {
            Some(
                Url::from_file_path(self.desktop_info.path.clone())
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
        desktop_info: DesktopInfo,
    ) -> Self {
        Self {
            id,
            toplevels,
            desktop_info,
        }
    }

    fn as_icon(
        &self,
        applet_helper: &CosmicAppletHelper,
        rectangle_tracker: Option<&RectangleTracker<u32>>,
        interaction_enabled: bool,
    ) -> Element<'_, Message> {
        let DockItem {
            toplevels,
            desktop_info,
            id,
            ..
        } = self;

        let cosmic_icon = cosmic::widget::icon(
            Path::new(&desktop_info.icon),
            applet_helper.suggested_size().0,
        );

        let dot_radius = 2;
        let dots = (0..toplevels.len())
            .into_iter()
            .map(|_| {
                container(vertical_space(Length::Units(0)))
                    .padding(dot_radius)
                    .style(<<CosmicAppList as cosmic::iced::Application>::Theme as container::StyleSheet>::Style::Custom(
                        |theme| container::Appearance {
                            text_color: Some(Color::TRANSPARENT),
                            background: Some(Background::Color(
                                theme.cosmic().on_bg_color().into(),
                            )),
                            border_radius: 4.0,
                            border_width: 0.0,
                            border_color: Color::TRANSPARENT,
                        },
                    ))
                    .into()
            })
            .collect_vec();
        let icon_wrapper = match applet_helper.anchor {
            PanelAnchor::Left => row(vec![column(dots).spacing(4).into(), cosmic_icon.into()])
                .align_items(iced::Alignment::Center)
                .spacing(4)
                .into(),
            PanelAnchor::Right => row(vec![cosmic_icon.into(), column(dots).spacing(4).into()])
                .align_items(iced::Alignment::Center)
                .spacing(4)
                .into(),
            PanelAnchor::Top => column(vec![row(dots).spacing(4).into(), cosmic_icon.into()])
                .align_items(iced::Alignment::Center)
                .spacing(4)
                .into(),
            PanelAnchor::Bottom => column(vec![cosmic_icon.into(), row(dots).spacing(4).into()])
                .align_items(iced::Alignment::Center)
                .spacing(4)
                .into(),
        };

        let mut icon_button = cosmic::widget::button(Button::Text)
            .custom(vec![icon_wrapper])
            .padding(8);
        let icon_button = if interaction_enabled {
            dnd_source(
                mouse_listener(
                    icon_button
                        .on_press(
                            toplevels
                                .first()
                                .map(|t| Message::Activate(t.0.clone()))
                                .unwrap_or_else(|| Message::Exec(desktop_info.exec.clone())),
                        )
                        .width(Length::Shrink)
                        .height(Length::Shrink),
                )
                .on_right_release(Message::Popup(desktop_info.id.clone())),
            )
            .on_drag(Message::StartDrag(desktop_info.id.clone()))
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
    theme: Theme,
    popup: Option<(window::Id, DockItem)>,
    surface_id_ctr: u32,
    subscription_ctr: u32,
    item_ctr: u32,
    active_list: Vec<DockItem>,
    favorite_list: Vec<DockItem>,
    dnd_source: Option<(window::Id, DockItem, DndAction)>,
    config: AppListConfig,
    toplevel_sender: Option<Sender<ToplevelRequest>>,
    applet_helper: CosmicAppletHelper,
    seat: Option<WlSeat>,
    rectangle_tracker: Option<RectangleTracker<u32>>,
    rectangles: HashMap<u32, iced::Rectangle>,
    dnd_offer: Option<DndOffer>,
    is_listening_for_dnd: bool,
}

// TODO DnD after sctk merges DnD
#[derive(Debug, Clone)]
enum Message {
    Toplevel(ToplevelUpdate),
    Favorite(String),
    UnFavorite(String),
    Popup(String),
    ClosePopup,
    Activate(ZcosmicToplevelHandleV1),
    Exec(String),
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
}

#[derive(Debug, Clone, Default)]
struct DesktopInfo {
    id: String,
    icon: PathBuf,
    exec: String,
    name: String,
    path: PathBuf,
}

fn desktop_info_for_app_ids(mut app_ids: Vec<String>) -> Vec<DesktopInfo> {
    let mut ret = freedesktop_desktop_entry::Iter::new(freedesktop_desktop_entry::default_paths())
        .filter_map(|path| {
            std::fs::read_to_string(&path).ok().and_then(|input| {
                DesktopEntry::decode(&path, &input).ok().and_then(|de| {
                    if let Some(i) = app_ids
                        .iter()
                        .position(|s| s == de.appid || s.eq(&de.name(None).unwrap_or_default()))
                    {
                        freedesktop_icons::lookup(de.icon().unwrap_or(de.appid))
                            .with_size(128)
                            .with_cache()
                            .find()
                            .map(|buf| DesktopInfo {
                                id: app_ids.remove(i),
                                icon: buf,
                                exec: de.exec().unwrap_or_default().to_string(),
                                name: de.name(None).unwrap_or_default().to_string(),
                                path: path.clone(),
                            })
                    } else {
                        None
                    }
                })
            })
        })
        .collect_vec();
    ret.append(
        &mut app_ids
            .into_iter()
            .map(|id| DesktopInfo {
                id,
                ..Default::default()
            })
            .collect_vec(),
    );
    ret
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
    let pos_in_list = pos_in_list * total_len as f32;
    let index = if list_len == 0 {
        0
    } else {
        if pos_in_list < item_size / 2.0 {
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
        }
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

impl Application for CosmicAppList {
    type Message = Message;
    type Theme = Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = config::AppListConfig::load().unwrap_or_default();
        let mut favorite_ctr = 0;
        let self_ = CosmicAppList {
            favorite_list: desktop_info_for_app_ids(config.favorites.clone())
                .into_iter()
                .map(|e| {
                    favorite_ctr += 1;
                    DockItem {
                        id: favorite_ctr,
                        toplevels: Default::default(),
                        desktop_info: e,
                    }
                })
                .collect(),
            config,
            ..Default::default()
        };

        (self_, Command::none())
    }

    fn title(&self) -> String {
        config::APP_ID.to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
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

                    self.surface_id_ctr += 1;
                    let new_id = window::Id::new(self.surface_id_ctr);
                    self.popup = Some((new_id, toplevel_group.clone()));

                    let mut popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
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
                    return get_popup(popup_settings);
                }
            }
            Message::Favorite(id) => {
                if let Some(i) = self
                    .active_list
                    .iter()
                    .position(|t| t.desktop_info.id == id || t.desktop_info.name == id)
                {
                    let entry = self.active_list.remove(i);
                    self.favorite_list.push(entry);
                }

                let _ = self.config.add_favorite(id);
                if let Some((popup_id, _toplevel)) = self.popup.take() {
                    return destroy_popup(popup_id);
                }
            }
            Message::UnFavorite(id) => {
                let _ = self.config.remove_favorite(id.clone());
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
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p.0);
                }
                if let (Some(tx), Some(seat)) = (self.toplevel_sender.as_ref(), self.seat.as_ref())
                {
                    let _ = tx.send(ToplevelRequest::Activate(handle, seat.clone()));
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
                        if let Some(tx) = self.toplevel_sender.as_ref() {
                            let _ = tx.send(ToplevelRequest::Quit(handle.clone()));
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
                            let _ = self.config.remove_favorite(t.desktop_info.id.clone());
                            Some((true, t))
                        } else {
                            None
                        }
                    })
                {
                    self.surface_id_ctr += 1;
                    let icon_id = window::Id::new(self.surface_id_ctr);
                    self.dnd_source = Some((icon_id, toplevel_group.clone(), DndAction::empty()));
                    return start_drag(
                        vec![MIME_TYPE.to_string()],
                        if is_favorite {
                            DndAction::all()
                        } else {
                            DndAction::Copy
                        },
                        window::Id::new(0),
                        Some(DndIcon::Custom(icon_id)),
                        Box::new(toplevel_group.clone()),
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
                let item_size = self.applet_helper.suggested_size().0;
                let pos_in_list = match self.applet_helper.anchor {
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
                    let item_size = self.applet_helper.suggested_size().0;
                    let pos_in_list = match self.applet_helper.anchor {
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
                    if let Some(di) = std::fs::read_to_string(&file_path).ok().and_then(|input| {
                        DesktopEntry::decode(&file_path, &input)
                            .ok()
                            .and_then(|de| {
                                freedesktop_icons::lookup(de.icon().unwrap_or(de.appid))
                                    .with_size(128)
                                    .with_cache()
                                    .find()
                                    .map(|buf| DesktopInfo {
                                        id: de.id().to_string(),
                                        icon: buf,
                                        exec: de.exec().unwrap_or_default().to_string(),
                                        name: de.name(None).unwrap_or_default().to_string(),
                                        path: file_path.clone(),
                                    })
                            })
                    }) {
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
                    let _ = self.config.add_favorite(dock_item.desktop_info.id.clone());
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
                }
                return finish_dnd();
            }
            Message::Toplevel(event) => {
                match event {
                    ToplevelUpdate::AddToplevel(handle, info) => {
                        if info.app_id.is_empty() {
                            return Command::none();
                        }
                        if let Some(t) = self
                            .active_list
                            .iter_mut()
                            .chain(self.favorite_list.iter_mut())
                            .find(|DockItem { desktop_info, .. }| {
                                desktop_info.id == info.app_id || desktop_info.name == info.app_id
                            })
                        {
                            t.toplevels.push((handle, info));
                        } else {
                            let desktop_info =
                                desktop_info_for_app_ids(vec![info.app_id.clone()]).remove(0);
                            self.item_ctr += 1;
                            self.active_list.push(DockItem {
                                id: self.item_ctr,
                                toplevels: vec![(handle, info)],
                                desktop_info,
                            });
                        }
                    }
                    ToplevelUpdate::Init(tx) => {
                        self.toplevel_sender.replace(tx);
                    }
                    ToplevelUpdate::Finished => {
                        self.subscription_ctr += 1;
                        for t in &mut self.favorite_list {
                            t.toplevels.clear();
                        }
                        self.active_list.clear();
                    }
                    ToplevelUpdate::RemoveToplevel(handle) => {
                        for t in self
                            .active_list
                            .iter_mut()
                            .chain(self.favorite_list.iter_mut())
                        {
                            t.toplevels.retain(|(t_handle, _)| t_handle != &handle);
                        }
                        self.active_list.retain(|t| !t.toplevels.is_empty());
                    }
                    ToplevelUpdate::UpdateToplevel(handle, info) => {
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
                }
            }
            Message::NewSeat(s) => {
                self.seat.replace(s);
            }
            Message::RemovedSeat(_) => {
                self.seat.take();
            }
            Message::Exec(exec_str) => {
                let mut exec = shlex::Shlex::new(&exec_str);
                let mut cmd = match exec.next() {
                    Some(cmd) if !cmd.contains('=') => tokio::process::Command::new(cmd),
                    _ => return Command::none(),
                };
                for arg in exec {
                    // TODO handle "%" args here if necessary?
                    if !arg.starts_with('%') {
                        cmd.arg(arg);
                    }
                }
                let _ = cmd.spawn();
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
        }
        Command::none()
    }

    fn view(&self, id: window::Id) -> Element<Message> {
        let is_horizontal = match self.applet_helper.anchor {
            PanelAnchor::Top | PanelAnchor::Bottom => true,
            PanelAnchor::Left | PanelAnchor::Right => false,
        };
        if let Some((_, item, _)) = self.dnd_source.as_ref().filter(|s| s.0 == id) {
            return cosmic::widget::icon(
                Path::new(&item.desktop_info.icon),
                self.applet_helper.suggested_size().0,
            )
            .into();
        }
        if let Some((
            _popup_id,
            DockItem {
                toplevels,
                desktop_info,
                ..
            },
        )) = self.popup.as_ref().filter(|p| id == p.0)
        {
            let is_favorite = self.config.favorites.contains(&desktop_info.id)
                || self.config.favorites.contains(&desktop_info.name);

            let mut content = column![
                iced::widget::text(&desktop_info.name).horizontal_alignment(Horizontal::Center),
                cosmic::widget::button(Button::Text)
                    .custom(vec![iced::widget::text(fl!("new-window")).into()])
                    .on_press(Message::Exec(desktop_info.exec.clone())),
            ]
            .padding(8)
            .spacing(4)
            .align_items(Alignment::Center);
            if !toplevels.is_empty() {
                let mut list_col = column![];
                for (handle, info) in toplevels {
                    let title = if info.title.len() > 20 {
                        format!("{:.24}...", &info.title)
                    } else {
                        info.title.clone()
                    };
                    list_col = list_col.push(
                        cosmic::widget::button(Button::Text)
                            .custom(vec![iced::widget::text(title).into()])
                            .on_press(Message::Activate(handle.clone())),
                    );
                }
                content = content.push(divider::horizontal::light());
                content = content.push(list_col);
                content = content.push(divider::horizontal::light());
            }
            content = content.push(if is_favorite {
                cosmic::widget::button(Button::Text)
                    .custom(vec![iced::widget::text(fl!("unfavorite")).into()])
                    .on_press(Message::UnFavorite(desktop_info.id.clone()))
            } else {
                cosmic::widget::button(Button::Text)
                    .custom(vec![iced::widget::text(fl!("favorite")).into()])
                    .on_press(Message::Favorite(desktop_info.id.clone()))
            });

            content = match toplevels.len() {
                0 => content,
                1 => content.push(
                    cosmic::widget::button(Button::Text)
                        .custom(vec![iced::widget::text(fl!("quit")).into()])
                        .on_press(Message::Quit(desktop_info.id.clone())),
                ),
                _ => content.push(
                    cosmic::widget::button(Button::Text)
                        .custom(vec![iced::widget::text(&fl!("quit-all")).into()])
                        .on_press(Message::Quit(desktop_info.id.clone())),
                ),
            };
            return self.applet_helper.popup_container(content).into();
        }

        let mut favorites: Vec<_> = self
            .favorite_list
            .iter()
            .map(|dock_item| {
                dock_item.as_icon(
                    &self.applet_helper,
                    self.rectangle_tracker.as_ref(),
                    self.popup.is_none(),
                )
            })
            .collect();

        if let Some((item, index)) = self
            .dnd_offer
            .as_ref()
            .and_then(|o| o.dock_item.as_ref().map(|item| (item, o.preview_index)))
        {
            favorites.insert(index, item.as_icon(&self.applet_helper, None, false));
        } else if self.is_listening_for_dnd && self.favorite_list.is_empty() {
            // show star indicating favorite_list is drag target
            favorites.push(
                container(cosmic::widget::icon(
                    "starred-symbolic.symbolic",
                    self.applet_helper.suggested_size().0,
                ))
                .padding(8)
                .into(),
            );
        }

        let active: Vec<_> = self
            .active_list
            .iter()
            .map(|dock_item| {
                dock_item.as_icon(
                    &self.applet_helper,
                    self.rectangle_tracker.as_ref(),
                    self.popup.is_none(),
                )
            })
            .collect();

        let (w, h, favorites, active, divider) = if is_horizontal {
            (
                Length::Fill,
                Length::Shrink,
                dnd_listener(row(favorites)),
                row(active).into(),
                vertical_rule(1).into(),
            )
        } else {
            (
                Length::Shrink,
                Length::Fill,
                dnd_listener(column(favorites)),
                column(active).into(),
                divider::horizontal::light().into(),
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
                |x, y| Message::DndMotion(x, y)
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
            vec![cosmic::widget::icon(
                "com.system76.CosmicAppList",
                self.applet_helper.suggested_size().0,
            )
            .into()]
        };

        let content = match &self.applet_helper.anchor {
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
        if self.popup.is_some() {
            mouse_listener(content)
                .on_right_release(Message::ClosePopup)
                .on_press(Message::ClosePopup)
                .into()
        } else {
            content.into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            toplevel_subscription(self.subscription_ctr).map(|(_, event)| Message::Toplevel(event)),
            events_with(|e, _| match e {
                native::Event::PlatformSpecific(event::PlatformSpecific::Wayland(
                    event::wayland::Event::Seat(e, seat),
                )) => match e {
                    event::wayland::SeatEvent::Enter => Some(Message::NewSeat(seat)),
                    event::wayland::SeatEvent::Leave => Some(Message::RemovedSeat(seat)),
                },
                // XXX Must be done to catch a finished drag after the source is removed
                // (for now, the source is removed when the drag starts)
                native::Event::PlatformSpecific(event::PlatformSpecific::Wayland(
                    event::wayland::Event::DataSource(
                        event::wayland::DataSourceEvent::DndFinished
                        | event::wayland::DataSourceEvent::Cancelled,
                    ),
                )) => Some(Message::DragFinished),
                native::Event::PlatformSpecific(event::PlatformSpecific::Wayland(
                    event::wayland::Event::DndOffer(event::wayland::DndOfferEvent::Enter {
                        mime_types,
                        ..
                    }),
                )) => {
                    if mime_types.iter().any(|m| m == MIME_TYPE) {
                        Some(Message::StartListeningForDnd)
                    } else {
                        None
                    }
                }
                native::Event::PlatformSpecific(event::PlatformSpecific::Wayland(
                    event::wayland::Event::DndOffer(
                        event::wayland::DndOfferEvent::Leave
                        | event::wayland::DndOfferEvent::DropPerformed,
                    ),
                )) => Some(Message::StopListeningForDnd),
                _ => None,
            }),
            rectangle_tracker_subscription(0).map(|(_, update)| Message::Rectangle(update)),
        ])
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: window::Id) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
