use std::ffi::OsStr;
use std::path::PathBuf;

use crate::config;
use crate::config::AppListConfig;
use crate::toplevel_subscription::toplevel_subscription;
use crate::toplevel_subscription::ToplevelRequest;
use crate::toplevel_subscription::ToplevelUpdate;
use calloop::channel::Sender;
use cctk::toplevel_info::ToplevelInfo;
use cctk::wayland_client::protocol::wl_seat::WlSeat;
use cosmic::applet::CosmicAppletHelper;
use cosmic::iced;
use cosmic::iced::wayland::popup::destroy_popup;
use cosmic::iced::wayland::popup::get_popup;
use cosmic::iced::wayland::SurfaceIdWrapper;
use cosmic::iced::widget::{column, row};
use cosmic::iced::{executor, window, Application, Command, Subscription};
use cosmic::iced_native::subscription::events_with;
use cosmic::iced_style::application::{self, Appearance};
use cosmic::iced_style::Color;
use cosmic::theme::Button;
use cosmic::widget::{horizontal_rule, vertical_rule};
use cosmic::{Element, Theme};
use cosmic_panel_config::PanelAnchor;
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1;
use freedesktop_desktop_entry::DesktopEntry;
use iced::wayland::window::resize_window;
use iced::widget::container;
use iced::widget::horizontal_space;
use iced::widget::svg;
use iced::widget::Image;
use iced::Alignment;
use iced::Background;
use iced::Length;
use itertools::Itertools;

pub fn run() -> cosmic::iced::Result {
    let helper = CosmicAppletHelper::default();
    CosmicAppList::run(helper.window_settings())
}

#[derive(Debug, Clone, Default)]
struct Toplevel {
    toplevels: Vec<(ZcosmicToplevelHandleV1, ToplevelInfo)>,
    desktop_info: DesktopInfo,
}

#[derive(Clone, Default)]
struct CosmicAppList {
    theme: Theme,
    popup: Option<window::Id>,
    id_ctr: u32,
    subscription_ctr: u32,
    toplevel_list: Vec<Toplevel>,
    config: AppListConfig,
    toplevel_sender: Option<Sender<ToplevelRequest>>,
    applet_helper: CosmicAppletHelper,
    seat: Option<WlSeat>,
}

// TODO DnD after sctk merges DnD
#[derive(Debug, Clone)]
enum Message {
    Toplevel(ToplevelUpdate),
    Favorite(String),
    UnFavorite(String),
    TogglePopup(usize),
    Activate(ZcosmicToplevelHandleV1),
    Exec(String),
    Quit(ZcosmicToplevelHandleV1),
    Errored(String),
    Ignore,
    NewSeat(WlSeat),
    RemovedSeat(WlSeat),
}

#[derive(Debug, Clone, Default)]
struct DesktopInfo {
    id: String,
    icon: PathBuf,
    exec: String,
    name: String,
}

fn desktop_info_for_app_ids(mut app_ids: Vec<String>) -> Vec<DesktopInfo> {
    let mut ret = freedesktop_desktop_entry::Iter::new(freedesktop_desktop_entry::default_paths())
        .filter_map(|path| {
            std::fs::read_to_string(&path).ok().and_then(|input| {
                DesktopEntry::decode(&path, &input).ok().and_then(|de| {
                    if let Some(i) = app_ids.iter().position(|s| s == de.appid || s.eq(&de.name(None).unwrap_or_default())) {
                        let id = app_ids.remove(i);
                        freedesktop_icons::lookup(de.icon().unwrap_or(de.appid))
                            .with_size(128)
                            .with_cache()
                            .find()
                            .map(|buf| DesktopInfo {id, icon: buf, exec: de.exec().unwrap_or_default().to_string(), name: de.name(None).unwrap_or_default().to_string()})
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
            .map(|id| DesktopInfo { id, ..Default::default() })
            .collect_vec(),
    );
    ret
}

impl Application for CosmicAppList {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = config::AppListConfig::load().unwrap_or_default();
        (
            CosmicAppList {
                toplevel_list: desktop_info_for_app_ids(config.favorites.clone())
                    .into_iter()
                    .map(|e| Toplevel {
                        toplevels: Default::default(),
                        desktop_info: e
                    })
                    .collect(),
                config,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        config::APP_ID.to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Errored(_) => {
                // TODO log errors
            }
            Message::TogglePopup(_) => {
                if let Some(p) = self.popup.take() {
                    return destroy_popup(p);
                } else {
                    self.id_ctr += 1;
                    let new_id = window::Id::new(self.id_ctr);
                    self.popup.replace(new_id);

                    let popup_settings = self.applet_helper.get_popup_settings(
                        window::Id::new(0),
                        new_id,
                        (400, 240),
                        None,
                        None,
                    );
                    return get_popup(popup_settings);
                }
            }
            Message::Favorite(id) => {
                let _ = self.config.add_favorite(id);
            }
            Message::UnFavorite(id) => {
                let _ = self.config.remove_favorite(id);
            }
            Message::Activate(handle) => {
                if let (Some(tx), Some(seat)) = (self.toplevel_sender.as_ref(), self.seat.as_ref()) {
                    let _ = tx.send(ToplevelRequest::Activate(handle, seat.clone()));
                }
            }
            Message::Quit(handle) => {
                if let Some(tx) = self.toplevel_sender.as_ref() {
                    let _ = tx.send(ToplevelRequest::Quit(handle));
                }
            }
            Message::Toplevel(event) => {
                // dbg!(&self.toplevel_list);
                match event {
                    ToplevelUpdate::AddToplevel(handle, info) => {
                        if info.app_id == "" {
                            return Command::none();
                        }
                        if let Some(i) = self
                            .toplevel_list
                            .iter()
                            .position(|Toplevel { desktop_info, .. }| &desktop_info.id == &info.app_id)
                        {
                            self.toplevel_list[i].toplevels.push((handle, info));
                        } else {
                            let desktop_info =
                                desktop_info_for_app_ids(vec![info.app_id.clone()]).remove(0);

                            self.toplevel_list.push(Toplevel {
                                toplevels: vec![(handle, info)],
                                desktop_info
                            });
                            // TODO better way of setting window size?
                            let pixel_size = self.applet_helper.suggested_icon_size();
                            let padding = 8;
                            let dot_size = 4;
                            let spacing = 4;
                            let length = self
                                .toplevel_list
                                .iter()
                                .map(|t| {
                                    (pixel_size + 2 * padding)
                                        .max((dot_size + spacing) * t.toplevels.len() as u16)
                                        as u32
                                        + spacing as u32
                                })
                                .sum();
                            let thickness = (pixel_size + 2 * padding + dot_size + spacing) as u32;
                            let (w, h) = match self.applet_helper.anchor {
                                PanelAnchor::Left | PanelAnchor::Right => (thickness, length),
                                PanelAnchor::Top | PanelAnchor::Bottom => (length, thickness),
                            };
                            return resize_window(window::Id::new(0), w, h);
                        }
                    }
                    ToplevelUpdate::Init(tx) => {
                        self.toplevel_sender.replace(tx);
                    }
                    ToplevelUpdate::Finished => {
                        self.subscription_ctr += 1;
                        for t in &mut self.toplevel_list {
                            t.toplevels.clear();
                        }
                    }
                    ToplevelUpdate::RemoveToplevel(handle) => {
                        if let Some(i) = self.toplevel_list.iter_mut().position(
                            |Toplevel {
                                 toplevels, desktop_info, ..
                             }| {
                                if let Some(ret) = toplevels.iter().position(|t| &t.0 == &handle) {
                                    toplevels.remove(ret);
                                    toplevels.is_empty() && self.config.favorites.contains(&desktop_info.id)
                                } else {
                                    false
                                }
                            },
                        ) {
                            self.toplevel_list.remove(i);
                        }
                        // TODO better way of setting window size?
                        let pixel_size = self.applet_helper.suggested_icon_size();
                        let padding = 8;
                        let dot_size = 4;
                        let spacing = 4;
                        let length = self
                            .toplevel_list
                            .iter()
                            .map(|t| {
                                (pixel_size + 2 * padding)
                                    .max((dot_size + spacing) * t.toplevels.len() as u16)
                                    as u32
                                    + spacing as u32
                            })
                            .sum();
                        let thickness = (pixel_size + 2 * padding + dot_size + spacing) as u32;
                        let (w, h) = match self.applet_helper.anchor {
                            PanelAnchor::Left | PanelAnchor::Right => (thickness, length),
                            PanelAnchor::Top | PanelAnchor::Bottom => (length, thickness),
                        };
                        return resize_window(window::Id::new(0), w, h);
                    }
                    ToplevelUpdate::UpdateToplevel(handle, info) => {
                        // TODO probably want to make sure it is removed
                        if info.app_id == "" {
                            return Command::none();
                        }
                        'toplevel_loop: for toplevel_list in &mut self.toplevel_list {
                            for (t_handle, t_info) in &mut toplevel_list.toplevels {
                                if &handle == t_handle {
                                    *t_info = info;
                                    break 'toplevel_loop;
                                }
                            }
                        }
                        // TODO better way of setting window size?
                        let pixel_size = self.applet_helper.suggested_icon_size();
                        let padding = 8;
                        let dot_size = 4;
                        let spacing = 4;
                        let length = self
                            .toplevel_list
                            .iter()
                            .map(|t| {
                                (pixel_size + 2 * padding)
                                    .max((dot_size + spacing) * t.toplevels.len() as u16)
                                    as u32
                                    + spacing as u32
                            })
                            .sum();
                        let thickness = (pixel_size + 2 * padding + dot_size + spacing) as u32;
                        let (w, h) = match self.applet_helper.anchor {
                            PanelAnchor::Left | PanelAnchor::Right => (thickness, length),
                            PanelAnchor::Top | PanelAnchor::Bottom => (length, thickness),
                        };
                        return resize_window(window::Id::new(0), w, h);
                    }
                }
            }
            Message::Ignore => {}
            Message::NewSeat(s) => {
                self.seat.replace(s);
            },
            Message::RemovedSeat(_) => {
                self.seat.take();
            },
            Message::Exec(exec_str) => {
                let mut exec = shlex::Shlex::new(&exec_str);
                let mut cmd = match exec.next() {
                    Some(cmd) if !cmd.contains("=") => tokio::process::Command::new(cmd),
                    _ => return Command::none(),
                };
                for arg in exec {
                    // TODO handle "%" args here if necessary?
                    if !arg.starts_with("%") {
                        cmd.arg(arg);
                    }
                }
                let _ = cmd.spawn();
            },
        }
        Command::none()
    }

    fn view(&self, id: SurfaceIdWrapper) -> Element<Message> {
        match id {
            SurfaceIdWrapper::LayerSurface(_) => unimplemented!(),
            SurfaceIdWrapper::Window(_) => {
                let (favorites, running) = self.toplevel_list.iter().enumerate().fold(
                    (Vec::new(), Vec::new()),
                    |(mut favorites, mut running),
                     (
                        i,
                        Toplevel {
                            toplevels,
                            desktop_info
                        },
                    )| {
                        let icon = if desktop_info.icon.extension() == Some(&OsStr::new("svg")) {
                            let handle = svg::Handle::from_path(&desktop_info.icon);
                            svg::Svg::new(handle)
                                .width(Length::Units(self.applet_helper.suggested_icon_size()))
                                .height(Length::Units(self.applet_helper.suggested_icon_size()))
                                .into()
                        } else {
                            Image::new(&desktop_info.icon)
                                .width(Length::Units(self.applet_helper.suggested_icon_size()))
                                .height(Length::Units(self.applet_helper.suggested_icon_size()))
                                .into()
                        };
                        let dot_radius = 2;
                        let dots = (0..toplevels.len())
                            .into_iter()
                            .map(|_| {
                                container(horizontal_space(Length::Units(0)))
                                    .padding(dot_radius)
                                    .style(<Self::Theme as container::StyleSheet>::Style::Custom(
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

                        let icon_wrapper = match &self.applet_helper.anchor {
                            PanelAnchor::Left => row(vec![column(dots).spacing(4).into(), icon])
                                .align_items(iced::Alignment::Center)
                                .spacing(4)
                                .into(),
                            PanelAnchor::Right => row(vec![icon, column(dots).spacing(4).into()])
                                .align_items(iced::Alignment::Center)
                                .spacing(4)
                                .into(),
                            PanelAnchor::Top => column(vec![row(dots).spacing(4).into(), icon])
                                .align_items(iced::Alignment::Center)
                                .spacing(4)
                                .into(),
                            PanelAnchor::Bottom => column(vec![icon, row(dots).spacing(4).into()])
                                .align_items(iced::Alignment::Center)
                                .spacing(4)
                                .into(),
                        };
                        // TODO tooltip on hover
                        let icon_button = cosmic::widget::button(Button::Text)
                            .custom(vec![icon_wrapper])
                            .on_press(toplevels.first().map(|t| Message::Activate(t.0.clone())).unwrap_or_else(|| Message::Exec(desktop_info.exec.clone())))
                            .padding(8)
                            .into();
                        if self.config.favorites.contains(&desktop_info.id) || self.config.favorites.contains(&desktop_info.name) {
                            favorites.push(icon_button)
                        } else {
                            running.push(icon_button);
                        }
                        (favorites, running)
                    },
                );
                match &self.applet_helper.anchor {
                    PanelAnchor::Left | PanelAnchor::Right => {
                        column![column(favorites), horizontal_rule(1), column(running)]
                            .spacing(4)
                            .align_items(Alignment::Center)
                            .height(Length::Fill)
                            .width(Length::Fill)
                            .into()
                    }
                    PanelAnchor::Top | PanelAnchor::Bottom => {
                        row![row(favorites), vertical_rule(1), row(running)]
                            .spacing(4)
                            .align_items(Alignment::Center)
                            .height(Length::Fill)
                            .width(Length::Fill)
                            .into()
                    }
                }
            }
            SurfaceIdWrapper::Popup(_) => {
                todo!();
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            toplevel_subscription(self.subscription_ctr).map(|(_, event)| Message::Toplevel(event)),
            events_with(|e, status| match e {
                cosmic::iced_native::Event::PlatformSpecific(
                    cosmic::iced_native::event::PlatformSpecific::Wayland(
                        cosmic::iced_native::event::wayland::Event::Seat(e, seat),
                    ),
                ) => match e {
                    cosmic::iced_native::event::wayland::SeatEvent::Enter => {
                        Some(Message::NewSeat(seat))
                    },
                    cosmic::iced_native::event::wayland::SeatEvent::Leave => {
                        Some(Message::RemovedSeat(seat))
                    },
                },
                _ => None
            }),
        ])
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn close_requested(&self, _id: SurfaceIdWrapper) -> Self::Message {
        Message::Ignore
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| Appearance {
            background_color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }
}
