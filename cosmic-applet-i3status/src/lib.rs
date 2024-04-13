mod localize;
mod subprocess;

use crate::localize::localize;
use cosmic::app::Command;
use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::iced::Length;
use cosmic::iced_futures::Subscription;
use cosmic::iced_style::application;
use cosmic::iced_widget::{Column, Row};
use cosmic::{Element, Theme};
use subprocess::Output;
use swaybar_types::Block;
use tracing::{span, Level};

pub fn run() -> cosmic::iced::Result {
    localize();
    cosmic::applet::run::<I3status>(true, ())
}

#[derive(Default)]
struct I3status {
    blocks: Vec<Block>,
    core: cosmic::app::Core,
    text: String,
}

impl cosmic::Application for I3status {
    type Message = Output;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletI3status";

    fn init(core: cosmic::app::Core, _flags: ()) -> (Self, Command<Output>) {
        (
            Self {
                blocks: vec![],
                core,
                text: String::new(),
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

    fn update(&mut self, message: Output) -> Command<Output> {
        let span = span!(Level::TRACE, "I3status::update()");
        let _ = span.enter();
        match message {
            Output::Blocks(blocks) => {
                self.blocks = blocks;
                self.text = String::new();
            }
            Output::Raw(output) => {
                self.blocks = vec![];
                self.text = output;
            }
            Output::None => {}
        }
        Command::none()
    }

    fn subscription(&self) -> Subscription<Output> {
        let span = span!(Level::TRACE, "I3status::subscription()");
        let _ = span.enter();
        subprocess::child_process()
    }

    fn view(&self) -> Element<Output> {
        let theme = self.core.system_theme().cosmic();
        let space_xxs = theme.space_xxs();

        let children = if !self.blocks.is_empty() {
            self.blocks
                .iter()
                .map(|block| cosmic::iced_widget::text(&block.full_text).into())
                .collect::<Vec<Element<Output>>>()
        } else if !self.text.is_empty() {
            vec![cosmic::iced_widget::text(&self.text).into()]
        } else {
            vec![cosmic::iced_widget::text("no output").into()]
        };

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
