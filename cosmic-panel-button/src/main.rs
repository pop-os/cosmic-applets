use cosmic::iced_widget::text;
use cosmic::{app, iced, iced_style::application, theme::Theme};
use freedesktop_desktop_entry::DesktopEntry;
use std::{env, fs, process::Command};

#[derive(Clone, Default)]
struct Desktop {
    name: String,
    icon: Option<String>,
    exec: String,
}

struct Button {
    core: cosmic::app::Core,
    desktop: Desktop,
}

#[derive(Debug, Clone)]
enum Msg {
    Press,
}

impl cosmic::Application for Button {
    type Message = Msg;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Desktop;
    const APP_ID: &'static str = "com.system76.CosmicPanelButton";

    fn init(core: cosmic::app::Core, desktop: Desktop) -> (Self, app::Command<Msg>) {
        (Self { core, desktop }, app::Command::none())
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

    fn update(&mut self, message: Msg) -> app::Command<Msg> {
        match message {
            Msg::Press => {
                let _ = Command::new("sh").arg("-c").arg(&self.desktop.exec).spawn();
            }
        }
        app::Command::none()
    }

    fn view(&self) -> cosmic::Element<Msg> {
        // TODO icon?
        cosmic::widget::button(text(&self.desktop.name))
            .style(cosmic::theme::Button::Text)
            .on_press(Msg::Press)
            .into()
    }
}

pub fn main() -> iced::Result {
    let id = env::args()
        .nth(1)
        .expect("Requires desktop file id as argument.");

    let filename = format!("{id}.desktop");
    let mut desktop = None;
    for mut path in freedesktop_desktop_entry::default_paths() {
        path.push(&filename);
        if let Ok(bytes) = fs::read_to_string(&path) {
            if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                desktop =
                    Some(Desktop {
                        name: entry.name(None).map(|x| x.to_string()).unwrap_or_else(|| {
                            panic!("Desktop file '{filename}' doesn't have `Name`")
                        }),
                        icon: entry.icon().map(|x| x.to_string()),
                        exec: entry.exec().map(|x| x.to_string()).unwrap_or_else(|| {
                            panic!("Desktop file '{filename}' doesn't have `Exec`")
                        }),
                    });
                break;
            }
        }
    }
    let desktop = desktop.unwrap_or_else(|| {
        panic!("Failed to find valid desktop file '{filename}' in search paths")
    });
    cosmic::applet::run::<Button>(true, desktop)
}
