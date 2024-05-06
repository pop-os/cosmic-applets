fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();

    cosmic_applet_input_sources::run()
}
