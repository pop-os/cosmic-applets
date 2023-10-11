// TODO: work vertically

use cosmic::{app, iced, iced_style::application, Theme};

mod bar_widget;
use bar_widget::BarWidget;
mod protocol;

#[derive(Clone, Debug)]
enum Msg {
    Protocol(protocol::StatusLine),
    ClickEvent(protocol::ClickEvent),
}

struct App {
    core: app::Core,
    status_line: protocol::StatusLine,
}

impl cosmic::Application for App {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = "com.system76.CosmicAppletStatusLine";

    fn init(core: app::Core, _flags: ()) -> (Self, app::Command<Msg>) {
        (
            App {
                core,
                status_line: Default::default(),
            },
            iced::Command::none(),
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(app::applet::style())
    }

    fn subscription(&self) -> iced::Subscription<Msg> {
        protocol::subscription().map(Msg::Protocol)
    }

    fn update(&mut self, message: Msg) -> app::Command<Msg> {
        match message {
            Msg::Protocol(status_line) => {
                println!("{:?}", status_line);
                self.status_line = status_line;
            }
            Msg::ClickEvent(click_event) => {
                println!("{:?}", click_event);
                if self.status_line.click_events {
                    // TODO: pass click event to backend
                }
            }
        }
        iced::Command::none()
    }

    fn view(&self) -> cosmic::Element<Msg> {
        let (block_views, name_instance): (Vec<_>, Vec<_>) = self
            .status_line
            .blocks
            .iter()
            .map(|block| {
                (
                    block_view(block),
                    (block.name.as_deref(), block.instance.as_deref()),
                )
            })
            .unzip();
        BarWidget {
            row: iced::widget::row(block_views),
            name_instance,
            on_press: Msg::ClickEvent,
        }
        .into()
    }
}

// TODO seperator
fn block_view(block: &protocol::Block) -> cosmic::Element<Msg> {
    let theme = block
        .color
        .map(cosmic::theme::Text::Color)
        .unwrap_or(cosmic::theme::Text::Default);
    cosmic::widget::text(&block.full_text).style(theme).into()
}

fn main() -> iced::Result {
    app::applet::run::<App>(true, ())
}
