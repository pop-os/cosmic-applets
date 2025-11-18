// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::{
    Element, Task, app,
    applet::cosmic_panel_config::PanelAnchor,
    applet::token::subscription::{TokenRequest, TokenUpdate, activation_token_subscription},
    cctk::sctk::reexports::calloop,
    iced::{
        self, Subscription,
        platform_specific::shell::commands::popup::{destroy_popup, get_popup},
        window,
    },
    surface,
    widget::{container, mouse_area},
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::{components::status_menu, subscriptions::status_notifier_watcher};

#[derive(Clone, Debug)]
pub enum Msg {
    Closed(window::Id),
    // XXX don't use index (unique window id? or I guess that's created and destroyed)
    StatusMenu((usize, status_menu::Msg)),
    StatusNotifier(status_notifier_watcher::Event),
    TogglePopup(usize),
    Hovered(usize),
    Surface(surface::Action),
    ToggleOverflow,
    HoveredOverflow,
    Token(TokenUpdate),
}

#[derive(Default)]
pub(crate) struct App {
    core: app::Core,
    connection: Option<zbus::Connection>,
    menus: BTreeMap<usize, status_menu::State>,
    open_menu: Option<usize>,
    max_menu_id: usize,
    popup: Option<window::Id>,
    overflow_popup: Option<window::Id>,
    token_tx: Option<calloop::channel::Sender<TokenRequest>>,
}

static ICON_NAME_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

impl App {
    /// Get icon theme directories from XDG base directories.
    /// This respects XDG_DATA_DIRS and includes flatpak directories if they're configured.
    /// Implements the XDG Base Directory specification without external dependencies.
    fn get_icon_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // XDG_DATA_HOME/icons (defaults to ~/.local/share/icons)
        let data_home = std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            });

        if let Some(data_home) = data_home {
            let icons_dir = data_home.join("icons");
            if icons_dir.exists() {
                dirs.push(icons_dir);
            }
        }

        // XDG_DATA_DIRS/icons (defaults to /usr/local/share:/usr/share)
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());

        for data_dir in data_dirs.split(':') {
            let icons_dir = PathBuf::from(data_dir).join("icons");
            if icons_dir.exists() {
                dirs.push(icons_dir);
            }
        }

        // ~/.icons for backwards compatibility
        if let Ok(home) = std::env::var("HOME") {
            let home_icons = PathBuf::from(home).join(".icons");
            if home_icons.exists() {
                dirs.push(home_icons);
            }
        }

        dirs
    }

    /// Search for an app icon that matches the search term (case-insensitive).
    /// This searches through XDG icon directories to find app IDs.
    fn find_app_icon_fuzzy(search_term: &str) -> Option<String> {
        let search_lower = search_term.to_lowercase();
        const ICON_EXTENSIONS: &[&str] = &[".png", ".svg", ".jpg", ".xpm"];

        for icon_dir in Self::get_icon_directories() {
            // Check hicolor theme apps directory (most common location)
            let hicolor_dir = icon_dir.join("hicolor");
            if let Ok(entries) = std::fs::read_dir(&hicolor_dir) {
                for entry in entries.flatten() {
                    let size_dir = entry.path();
                    if !size_dir.is_dir() {
                        continue;
                    }

                    let apps_dir = size_dir.join("apps");
                    if let Ok(app_icons) = std::fs::read_dir(&apps_dir) {
                        for app_icon in app_icons.flatten() {
                            let file_name = app_icon.file_name();
                            let name_str = file_name.to_string_lossy();

                            let mut app_id = name_str.as_ref();
                            for ext in ICON_EXTENSIONS {
                                if let Some(stripped) = app_id.strip_suffix(ext) {
                                    app_id = stripped;
                                    break;
                                }
                            }

                            if app_id.to_lowercase().contains(&search_lower) {
                                return Some(app_id.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Try icon name variants and fuzzy search through XDG icon directories.
    /// This respects XDG_DATA_DIRS and properly searches all icon theme paths
    /// including flatpak directories if they're in the XDG paths.
    fn try_icon_name_variants(icon_name: &str, service_name: &str) -> Option<String> {
        let mut variants = Vec::new();

        for suffix in ["_tray_mono", "_tray", "_mono", "-tray", "-mono"] {
            if let Some(stripped) = icon_name.strip_suffix(suffix) {
                if !stripped.is_empty() && !variants.contains(&stripped.to_string()) {
                    variants.push(stripped.to_string());
                }
            }
        }

        // Try the last component of the service name (often the app ID)
        if let Some(last_component) = service_name.split('/').last() {
            if !last_component.is_empty() && !variants.contains(&last_component.to_string()) {
                variants.push(last_component.to_string());
            }
        }

        // Try fuzzy searching for app icons using each variant
        // This helps find icons like "com.valvesoftware.Steam" when searching for "steam"
        // We trust libcosmic's icon system to handle the actual icon lookup
        for variant in &variants {
            if let Some(app_id) = Self::find_app_icon_fuzzy(variant) {
                return Some(app_id);
            }
        }

        None
    }

    fn map_icon_name(icon_name: &str, service_name: &str) -> String {
        let cache = ICON_NAME_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

        if let Ok(cache_lock) = cache.lock() {
            if let Some(cached) = cache_lock.get(icon_name) {
                return cached
                    .as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| icon_name.to_string());
            }
        }

        let mapped_icon = Self::try_icon_name_variants(icon_name, service_name);

        if let Some(fallback_id) = mapped_icon {
            if let Ok(mut cache_lock) = cache.lock() {
                cache_lock.insert(icon_name.to_string(), Some(fallback_id.clone()));
            }
            return fallback_id;
        }

        if let Ok(mut cache_lock) = cache.lock() {
            cache_lock.insert(icon_name.to_string(), None);
        }

        icon_name.to_string()
    }

    fn get_mapped_icon_name(menu: &status_menu::State) -> Option<&'static str> {
        if menu.icon_name().is_empty() {
            None
        } else {
            let mapped = Self::map_icon_name(menu.icon_name(), menu.name());
            Some(Box::leak(mapped.into_boxed_str()))
        }
    }

    fn create_menu_icon_button(
        &self,
        menu: &status_menu::State,
    ) -> cosmic::widget::Button<'_, Msg> {
        let mapped_icon_name = Self::get_mapped_icon_name(menu);

        match menu.icon_pixmap() {
            Some(icon) if menu.icon_name().is_empty() => self
                .core
                .applet
                .icon_button_from_handle(icon.clone().symbolic(true)),
            _ if mapped_icon_name.is_some() => {
                self.core.applet.icon_button(mapped_icon_name.unwrap())
            }
            _ => self
                .core
                .applet
                .icon_button("application-x-executable-symbolic"),
        }
    }

    fn next_menu_id(&mut self) -> usize {
        self.max_menu_id += 1;
        self.max_menu_id
    }

    fn next_popup_id(&mut self) -> window::Id {
        window::Id::unique()
    }

    fn resize_window(&self) -> app::Task<Msg> {
        let icon_size = self.core.applet.suggested_size(true).0 as u32
            + self.core.applet.suggested_padding(true).1 as u32 * 2;
        let n = self.menus.len() as u32;
        window::resize(
            self.core.main_window_id().unwrap(),
            iced::Size::new(1.max(icon_size * n) as f32, icon_size as f32),
        )
    }

    fn overflow_index(&self) -> Option<usize> {
        let max_major_axis_len = self.core.applet.suggested_bounds.as_ref().map(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
            }
        })?;

        let button_total_size = self.core.applet.suggested_size(true).0
            + self.core.applet.suggested_padding(true).1 * 2;

        let menu_count = self.menus.len();

        let btn_count = max_major_axis_len / button_total_size as u32;
        if btn_count >= menu_count as u32 {
            None
        } else {
            Some(
                (btn_count.saturating_sub(1) as usize)
                    .min(menu_count)
                    .max(1),
            )
        }
    }

    fn view_overflow_popup(&self) -> cosmic::Element<'_, Msg> {
        // Render the overflow popup with the menus that are not shown in the main view
        let overflow_index = self.overflow_index().unwrap_or(0);
        let children = self.menus.iter().skip(overflow_index).map(|(id, menu)| {
            mouse_area(
                self.create_menu_icon_button(menu)
                    .on_press_down(Msg::TogglePopup(*id)),
            )
            .on_enter(Msg::Hovered(*id))
            .into()
        });
        let theme = self.core.system_theme();
        let cosmic = theme.cosmic();
        let _corners = cosmic.corner_radii;

        self.core
            .applet
            .popup_container(container(
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    Element::from(iced::widget::column(children))
                } else {
                    Element::from(iced::widget::row(children))
                },
            ))
            .into()
    }
}

impl cosmic::Application for App {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletStatusArea";

    fn init(core: app::Core, _flags: ()) -> (Self, app::Task<Msg>) {
        (
            Self {
                core,
                ..Self::default()
            },
            Task::none(),
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Msg) -> app::Task<Msg> {
        match message {
            Msg::Closed(surface) => {
                if self.popup == Some(surface) {
                    self.popup = None;
                    self.open_menu = None;
                }
                Task::none()
            }
            Msg::StatusMenu((id, msg)) => match self.menus.get_mut(&id) {
                Some(state) => state
                    .update(msg, id, self.token_tx.as_ref())
                    .map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg)))),
                None => Task::none(),
            },
            Msg::StatusNotifier(event) => match event {
                status_notifier_watcher::Event::Connected(connection) => {
                    self.connection = Some(connection);
                    Task::none()
                }
                status_notifier_watcher::Event::Registered(name) => {
                    let (state, cmd) = status_menu::State::new(name);
                    if let Some((id, m)) = self
                        .menus
                        .iter_mut()
                        .find(|(_, prev_state)| prev_state.name() == state.name())
                    {
                        *m = state;
                        let id = *id;
                        return cmd.map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg))));
                    }
                    let id = self.next_menu_id();
                    self.menus.insert(id, state);
                    app::Task::batch([
                        self.resize_window(),
                        cmd.map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg)))),
                    ])
                }
                status_notifier_watcher::Event::Unregistered(name) => {
                    if let Some((id, _menu)) =
                        self.menus.iter().find(|(_id, menu)| menu.name() == name)
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
                    eprintln!("Status notifier error: {err}");
                    Task::none()
                }
            },
            Msg::TogglePopup(id) => {
                self.open_menu = if self.open_menu.is_none() {
                    Some(id)
                } else {
                    None
                };
                if self.open_menu.is_some() {
                    self.menus[&id].opened();

                    let mut cmds = Vec::new();
                    if let Some(popup_id) = self.popup.take() {
                        cmds.push(destroy_popup(popup_id));
                    }
                    let popup_id = self.next_popup_id();
                    let i = self.menus.keys().position(|&i| i == id).unwrap();
                    let (i, parent) = self
                        .overflow_index()
                        .and_then(|overflow_i| {
                            if overflow_i <= i {
                                Some(i - overflow_i).zip(self.overflow_popup)
                            } else {
                                Some((i, self.core.main_window_id().unwrap()))
                            }
                        })
                        .unwrap_or((0, self.core.main_window_id().unwrap()));

                    let mut popup_settings = self
                        .core
                        .applet
                        .get_popup_settings(parent, popup_id, None, None, None);
                    self.popup = Some(popup_id);

                    if matches!(
                        self.core.applet.anchor,
                        PanelAnchor::Left | PanelAnchor::Right
                    ) {
                        let suggested_size = self.core.applet.suggested_size(false).1
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                    } else {
                        let suggested_size = self.core.applet.suggested_size(false).0
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                    }
                    cmds.push(get_popup(popup_settings));
                    return app::Task::batch(cmds);
                } else if let Some(popup_id) = self.popup {
                    self.menus[&id].closed();

                    return destroy_popup(popup_id);
                }
                Task::none()
            }
            Msg::Token(u) => match u {
                TokenUpdate::Init(tx) => {
                    self.token_tx = Some(tx);
                    return Task::none();
                }
                TokenUpdate::Finished => {
                    self.token_tx = None;
                    return Task::none();
                }
                TokenUpdate::ActivationToken { token, exec: id } => {
                    if let Some(((state, id), token)) = str::parse(&id)
                        .ok()
                        .and_then(|id: usize| self.menus.get_mut(&id).map(|m| (m, id)))
                        .zip(token)
                    {
                        return state
                            .update(
                                status_menu::Msg::ClickToken(token),
                                id,
                                self.token_tx.as_ref(),
                            )
                            .map(move |msg| cosmic::action::app(Msg::StatusMenu((id, msg))));
                    }
                    return Task::none();
                }
            },
            Msg::Hovered(id) => {
                let mut cmds = Vec::new();
                if let Some(old_id) = self.open_menu.take() {
                    if old_id != id {
                        if let Some(popup_id) = self.popup.take() {
                            cmds.push(destroy_popup(popup_id));
                        }
                        self.open_menu = Some(id);
                    } else {
                        self.open_menu = Some(old_id);
                        return Task::none();
                    }
                } else {
                    return Task::none();
                }
                let popup_id = self.next_popup_id();
                let i = self.menus.keys().position(|&i| i == id).unwrap();

                let (i, parent) = self
                    .overflow_index()
                    .and_then(|overflow_i| {
                        if overflow_i <= i {
                            Some(i - overflow_i).zip(self.overflow_popup)
                        } else {
                            Some((i, self.core.main_window_id().unwrap()))
                        }
                    })
                    .unwrap_or((0, self.core.main_window_id().unwrap()));

                let mut popup_settings = self
                    .core
                    .applet
                    .get_popup_settings(parent, popup_id, None, None, None);
                self.popup = Some(popup_id);

                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    let suggested_size = self.core.applet.suggested_size(false).1
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                } else {
                    let suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                }
                cmds.push(get_popup(popup_settings));
                Task::batch(cmds)
            }
            Msg::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
            Msg::ToggleOverflow => {
                if let Some(popup_id) = self.overflow_popup.take() {
                    self.popup = None;
                    self.open_menu = None;
                    return destroy_popup(popup_id);
                } else if let Some(overflow_index) = self.overflow_index() {
                    // If we don't have an overflow, create it
                    let popup_id = self.next_popup_id();
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        popup_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.close_with_children = false;

                    if matches!(
                        self.core.applet.anchor,
                        PanelAnchor::Left | PanelAnchor::Right
                    ) {
                        let suggested_size = self.core.applet.suggested_size(false).1
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.y =
                            overflow_index as i32 * suggested_size as i32;
                    } else {
                        let suggested_size = self.core.applet.suggested_size(false).0
                            + 2 * self.core.applet.suggested_padding(false).1;
                        popup_settings.positioner.anchor_rect.x =
                            overflow_index as i32 * suggested_size as i32;
                    }

                    self.overflow_popup = Some(popup_id);
                    return get_popup(popup_settings);
                } else {
                    return Task::none();
                }
            }
            Msg::HoveredOverflow => {
                let mut cmds = Vec::new();
                if self.overflow_popup.is_some() {
                    // If we already have an overflow popup, do nothing
                    return Task::none();
                } else if self.open_menu.is_some() {
                    // If we have an open menu, close it
                    if let Some(popup_id) = self.popup.take() {
                        cmds.push(destroy_popup(popup_id));
                    }
                } else {
                    return Task::none();
                }

                let popup_id = self.next_popup_id();
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    popup_id,
                    None,
                    None,
                    None,
                );
                self.popup = Some(popup_id);

                let Some(i) = self.overflow_index() else {
                    return Task::batch(cmds);
                };

                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    let suggested_size = self.core.applet.suggested_size(false).1
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.y = i as i32 * suggested_size as i32;
                } else {
                    let suggested_size = self.core.applet.suggested_size(false).0
                        + 2 * self.core.applet.suggested_padding(false).1;
                    popup_settings.positioner.anchor_rect.x = i as i32 * suggested_size as i32;
                }
                cmds.push(get_popup(popup_settings));
                Task::batch(cmds)
            }
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let mut subscriptions = Vec::new();

        subscriptions.push(status_notifier_watcher::subscription().map(Msg::StatusNotifier));

        for (id, menu) in &self.menus {
            subscriptions.push(menu.subscription().with(*id).map(Msg::StatusMenu));
        }
        subscriptions.push(activation_token_subscription(0).map(Msg::Token));

        iced::Subscription::batch(subscriptions)
    }

    fn view(&self) -> cosmic::Element<'_, Msg> {
        let overflow_index = self.overflow_index();

        let children = self
            .menus
            .iter()
            .take(overflow_index.unwrap_or(self.menus.len()))
            .map(|(id, menu)| {
                mouse_area(self.create_menu_icon_button(menu).on_press_down(
                    if menu.item.menu_proxy().is_some() {
                        Msg::TogglePopup(*id)
                    } else {
                        Msg::StatusMenu((*id, status_menu::Msg::Click(0, true)))
                    },
                ))
                .on_enter(Msg::Hovered(*id))
                .into()
            });

        self.core
            .applet
            .autosize_window(
                if matches!(
                    self.core.applet.anchor,
                    PanelAnchor::Left | PanelAnchor::Right
                ) {
                    Element::from(iced::widget::column(children))
                } else {
                    iced::widget::row(children)
                        .push_maybe(overflow_index.map(|_| {
                            mouse_area(
                                self.core
                                    .applet
                                    .icon_button(match self.core.applet.anchor {
                                        PanelAnchor::Bottom => "go-up-symbolic",
                                        PanelAnchor::Left => "go-next-symbolic",
                                        PanelAnchor::Right => "go-previous-symbolic",
                                        PanelAnchor::Top => "go-down-symbolic",
                                    })
                                    .on_press_down(Msg::ToggleOverflow),
                            )
                            .on_enter(Msg::HoveredOverflow)
                        }))
                        .into()
                },
            )
            .into()
    }

    fn view_window(&self, surface: window::Id) -> cosmic::Element<'_, Msg> {
        if self
            .overflow_popup
            .as_ref()
            .is_some_and(|id| *id == surface)
        {
            return self.view_overflow_popup();
        }

        let theme = self.core.system_theme();
        let cosmic = theme.cosmic();
        let corners = cosmic.corner_radii;
        let _pad = corners.radius_m[0];
        match self.open_menu {
            Some(id) => match self.menus.get(&id) {
                Some(menu) => self
                    .core
                    .applet
                    .popup_container(
                        container(menu.popup_view().map(move |msg| Msg::StatusMenu((id, msg))))
                            .padding([_pad, 0.]),
                    )
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
    cosmic::applet::run::<App>(())
}
