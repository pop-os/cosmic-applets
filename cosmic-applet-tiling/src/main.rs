use crate::window::Window;

mod localize;
mod wayland;
mod wayland_subscription;
mod window;

fn main() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Window>(false, ())
}
