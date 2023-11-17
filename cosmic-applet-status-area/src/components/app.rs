use cosmic::{
    app::{self, Command},
    iced::{
        self,
        wayland::{
            popup::{destroy_popup, get_popup},
            window::resize_window,
        },
        window, Subscription,
    },
    iced_style::application,
    Theme,
};
use std::collections::BTreeMap;

use crate::{components::status_menu, subscriptions::status_notifier_watcher};

// XXX copied from libcosmic
const APPLET_PADDING: u32 = 8;

#[derive(Clone, Debug)]
pub enum Msg {
    Closed(window::Id),
    // XXX don't use index (unique window id? or I guess that's created and destroyed)
    StatusMenu((usize, status_menu::Msg)),
    StatusNotifier(status_notifier_watcher::Event),
    TogglePopup(usize),
}

#[derive(Default)]
struct App {
    core: app::Core,
    connection: Option<zbus::Connection>,
    menus: BTreeMap<usize, status_menu::State>,
    open_menu: Option<usize>,
    max_menu_id: usize,
    max_popup_id: u128,
    popup: Option<window::Id>,
}

impl App {
    fn next_menu_id(&mut self) -> usize {
        self.max_menu_id += 1;
        self.max_menu_id
    }

    fn next_popup_id(&mut self) -> window::Id {
        self.max_popup_id += 1;
        window::Id(self.max_popup_id)
    }

    fn resize_window(&self) -> Command<Msg> {
        let icon_size = self.core.applet.suggested_size().0 as u32 + APPLET_PADDING * 2;
        let n = self.menus.len() as u32;
        resize_window(window::Id(0), 1.max(icon_size * n), icon_size)
    }
}

impl cosmic::Application for App {
    type Message = Msg;
    type Executor = iced::executor::Default;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletStatusArea";

    fn init(core: app::Core, _flags: ()) -> (Self, app::Command<Msg>) {
        (
            Self {
                core,
                ..Self::default()
            },
            Command::none(),
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> Command<Msg> {
        match message {
            Msg::Closed(surface) => {
                if self.popup == Some(surface) {
                    self.popup = None;
                    self.open_menu = None;
                }
                Command::none()
            }
            Msg::StatusMenu((id, msg)) => match self.menus.get_mut(&id) {
                Some(state) => state
                    .update(msg)
                    .map(move |msg| app::message::app(Msg::StatusMenu((id, msg)))),
                None => Command::none(),
            },
            Msg::StatusNotifier(event) => match event {
                status_notifier_watcher::Event::Connected(connection) => {
                    self.connection = Some(connection);
                    Command::none()
                }
                status_notifier_watcher::Event::Registered(name) => {
                    let (state, cmd) = status_menu::State::new(name);
                    let id = self.next_menu_id();
                    self.menus.insert(id, state);
                    Command::batch([
                        self.resize_window(),
                        cmd.map(move |msg| app::message::app(Msg::StatusMenu((id, msg)))),
                    ])
                }
                status_notifier_watcher::Event::Unregistered(name) => {
                    if let Some((id, _)) = self.menus.iter().find(|(_id, menu)| menu.name() == name)
                    {
                        let id = *id;
                        self.menus.remove(&id);
                        if self.open_menu == Some(id) {
                            self.open_menu = None;
                            if let Some(popup_id) = self.popup {
                                return destroy_popup(popup_id);
                            }
                        }
                    }
                    self.resize_window()
                }
                status_notifier_watcher::Event::Error(err) => {
                    eprintln!("Status notifier error: {}", err);
                    Command::none()
                }
            },
            Msg::TogglePopup(id) => {
                self.open_menu = if self.open_menu != Some(id) {
                    Some(id)
                } else {
                    None
                };
                // Reuse popup if a different menu is opened.
                // Had issue creating new one. Does it make a difference?
                if self.open_menu.is_some() {
                    if self.popup.is_none() {
                        let id = self.next_popup_id();
                        let popup_settings = self.core.applet.get_popup_settings(
                            window::Id(0),
                            id,
                            None,
                            None,
                            None,
                        );
                        self.popup = Some(id);
                        return get_popup(popup_settings);
                    }
                } else if let Some(id) = self.popup {
                    return destroy_popup(id);
                }
                Command::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subscriptions = Vec::new();

        subscriptions.push(status_notifier_watcher::subscription().map(Msg::StatusNotifier));

        for (id, menu) in self.menus.iter() {
            subscriptions.push(menu.subscription().with(*id).map(Msg::StatusMenu));
        }

        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        // XXX connect open event
        iced::widget::row(
            self.menus
                .iter()
                .map(|(id, menu)| {
                    self.core
                        .applet
                        .icon_button(menu.icon_name())
                        .on_press(Msg::TogglePopup(*id))
                        .into()
                })
                .collect(),
        )
        .into()
    }

    fn view_window(&self, _surface: window::Id) -> cosmic::Element<'_, Msg> {
        match self.open_menu {
            Some(id) => match self.menus.get(&id) {
                Some(menu) => self
                    .core
                    .applet
                    .popup_container(menu.popup_view().map(move |msg| Msg::StatusMenu((id, msg))))
                    .into(),
                None => unreachable!(),
            },
            None => iced::widget::text("").into(),
        }
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Msg> {
        Some(Msg::Closed(id))
    }
}

pub fn main() -> iced::Result {
    cosmic::applet::run::<App>(true, ())
}
