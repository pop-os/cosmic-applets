mod components;
mod subscriptions;

pub fn run() -> cosmic::iced::Result {
    components::app::main()
}
