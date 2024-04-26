mod localize;
pub(crate) mod wayland_handler;
pub(crate) mod wayland_subscription;
pub(crate) mod window_image;

use crate::localize::localize;
use cosmic::app::Command;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::cctk::cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1;
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::cctk::toplevel_info::ToplevelInfo;
use cosmic::desktop::DesktopEntryData;
use cosmic::iced::{widget::text, Length, Subscription};

use cosmic::iced_style::application;
use cosmic::iced_widget::{Column, Row};

use cosmic::widget::tooltip;
use cosmic::{Element, Theme};
use wayland_subscription::{
    ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<Minimize>(true, ())
}

#[derive(Default)]
struct Minimize {
    core: cosmic::app::Core,
    apps: Vec<(
        ZcosmicToplevelHandleV1,
        ToplevelInfo,
        DesktopEntryData,
        Option<WaylandImage>,
    )>,
    tx: Option<calloop::channel::Sender<WaylandRequest>>,
}

#[derive(Debug, Clone)]
enum Message {
    Wayland(WaylandUpdate),
    Activate(ZcosmicToplevelHandleV1),
}

impl cosmic::Application for Minimize {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletMinimize";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                core,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Wayland(update) => match update {
                WaylandUpdate::Init(tx) => {
                    self.tx = Some(tx);
                }
                WaylandUpdate::Finished => {
                    panic!("Wayland Subscription ended...")
                }
                WaylandUpdate::Toplevel(t) => match t {
                    ToplevelUpdate::Add(handle, info) | ToplevelUpdate::Update(handle, info) => {
                        let data = |id| {
                            cosmic::desktop::load_applications_for_app_ids(
                                None,
                                std::iter::once(id),
                                true,
                                false,
                            )
                            .remove(0)
                        };
                        if let Some(pos) = self.apps.iter_mut().position(|a| a.0 == handle) {
                            if self.apps[pos].1.app_id != info.app_id {
                                self.apps[pos].2 = data(&info.app_id)
                            }
                            self.apps[pos].1 = info;
                        } else {
                            let data = data(&info.app_id);
                            self.apps.push((handle, info, data, None));
                        }
                    }
                    ToplevelUpdate::Remove(handle) => self.apps.retain(|a| a.0 != handle),
                },
                WaylandUpdate::Image(handle, img) => {
                    if let Some(pos) = self.apps.iter().position(|a| a.0 == handle) {
                        self.apps[pos].3 = Some(img);
                    }
                }
            },
            Message::Activate(handle) => {
                if let Some(tx) = self.tx.as_ref() {
                    let _ = tx.send(WaylandRequest::Toplevel(ToplevelRequest::Activate(handle)));
                }
            }
        };
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        wayland_subscription::wayland_subscription().map(Message::Wayland)
    }

    fn view(&self) -> Element<Message> {
        let (width, _) = self.core.applet.suggested_size(false);
        let padding = self.core.applet.suggested_padding(false);
        let theme = self.core.system_theme().cosmic();
        let space_xxs = theme.space_xxs();
        let icon_buttons = self.apps.iter().map(|(handle, _, data, img)| {
            tooltip(
                Element::from(crate::window_image::WindowImage::new(
                    img.clone(),
                    &data.icon,
                    width as f32,
                    Message::Activate(handle.clone()),
                    padding,
                )),
                data.name.clone(),
                // tooltip::Position::FollowCursor,
                // FIXME tooltip fails to appear when created as indicated in design
                // maybe it should be a subsurface
                match self.core.applet.anchor {
                    PanelAnchor::Left => tooltip::Position::Right,
                    PanelAnchor::Right => tooltip::Position::Left,
                    PanelAnchor::Top => tooltip::Position::Bottom,
                    PanelAnchor::Bottom => tooltip::Position::Top,
                },
            )
            .snap_within_viewport(false)
            .text_shaping(text::Shaping::Advanced)
            .into()
        });

        // TODO optional dividers on ends if detects app list neighbor
        // not sure the best way to tell if there is an adjacent app-list

        if matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        ) {
            Row::with_children(icon_buttons)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        } else {
            Column::with_children(icon_buttons)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .into()
        }
    }
}
