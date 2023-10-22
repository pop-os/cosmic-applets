mod localize;
mod window;

use window::Window;

fn main() -> cosmic::iced::Result {
    localize::localize();

    cosmic::applet::run::<Window>(true, ())
}
