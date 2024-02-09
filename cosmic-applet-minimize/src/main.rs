mod localize;

use crate::localize::localize;
use cosmic::app::Command;
use cosmic::applet::token::subscription::{activation_token_subscription, TokenUpdate};
use cosmic::iced::{widget::text, Length, Subscription};
use cosmic::iced_style::application;
use cosmic::widget::cosmic_container;

use cosmic::{Element, Theme};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();

    // Prepare i18n
    localize();

    tracing::info!("Starting minimize applet with version {VERSION}");

    cosmic::applet::run::<Minimize>(false, ())
}

#[derive(Default)]
struct Minimize {
    core: cosmic::app::Core,
}

#[derive(Debug, Clone)]
enum Message {
    Token(TokenUpdate),
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
        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![activation_token_subscription(0).map(Message::Token)])
    }

    fn view(&self) -> Element<Message> {
        cosmic_container::container(text("X"))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
