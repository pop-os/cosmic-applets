mod localize;

use crate::localize::localize;
use cosmic::app::Command;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::iced::Length;

use cosmic::iced_style::application;
use cosmic::iced_widget::{Column, Row};

use cosmic::{Element, Theme};

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<I3status>(true, ())
}

#[derive(Default)]
struct I3status {
    core: cosmic::app::Core,
}

#[derive(Debug, Clone)]
enum Message {}

impl cosmic::Application for I3status {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletI3status";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Message>) {
        (Self { core }, Command::none())
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

    fn update(&mut self, _message: Message) -> Command<Message> {
        Command::none()
    }

    // TODO: subscribe? to new output from the `i3status` process
    // fn subscription(&self) -> Subscription<Message> {
    // }

    fn view(&self) -> Element<Message> {
        let theme = self.core.system_theme().cosmic();
        let space_xxs = theme.space_xxs();

        let label = cosmic::iced_widget::text("i3status here");
        let children = vec![label.into()];

        if matches!(
            self.core.applet.anchor,
            PanelAnchor::Top | PanelAnchor::Bottom
        ) {
            Row::with_children(children)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .padding([0, space_xxs])
                .into()
        } else {
            Column::with_children(children)
                .align_items(cosmic::iced_core::Alignment::Center)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .spacing(space_xxs)
                .padding([space_xxs, 0])
                .into()
        }
    }
}
