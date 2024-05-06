use cosmic::applet::cosmic_panel_config::PanelAnchor;
use cosmic::iced::Length;
use cosmic::iced_widget::{row, text};
use cosmic::widget::vertical_space;
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
        if matches!(
            self.core.applet.anchor,
            PanelAnchor::Left | PanelAnchor::Right
        ) && self.desktop.icon.is_some()
        {
            self.core
                .applet
                .icon_button(self.desktop.icon.as_ref().unwrap())
        } else {
            let content = row!(
                text(&self.desktop.name).size(14.0),
                vertical_space(Length::Fixed(
                    (self.core.applet.suggested_size(true).1
                        + 2 * self.core.applet.suggested_padding(true)) as f32
                ))
            )
            .align_items(iced::Alignment::Center);
            cosmic::widget::button(content)
                .padding([0, self.core.applet.suggested_padding(false)])
                .style(cosmic::theme::Button::AppletIcon)
        }
        .on_press(Msg::Press)
        .into()
    }
}

pub fn main() -> iced::Result {
    let mut desktop = None;
    if env::args().len() > 2
        || env::args()
            .nth(1)
            .expect("Requires a desktop id or --help for an argument.")
            == "--help"
    {
        let args: Vec<String> = env::args().collect();
        let mut exec = None;
        let mut name = None;
        let mut icon = None;

        for i in 1..args.len() {
            match args[i].as_str() {
                "-e" | "--exec" => {
                    exec = args.get(i + 1).map(|s| s.to_owned());
                }
                "-n" | "--name" => {
                    name = args.get(i + 1).map(|s| s.to_owned());
                }
                "-i" | "--icon" => {
                    icon = args.get(i + 1).map(|s| s.to_owned());
                }
                "-h" | "--help" => {
                    println!("cosmic-panel-button is a cosmic applet which creates a button to either run .desktop files or execute a command. \n");
                    println!("--exec and --name are required arguments. \n");
                    println!("-e, --exec <COMMAND>  Command to execute");
                    println!("-n, --name <NAME>     Name of the applet");
                    println!("-i, --icon <ICON>     Name of the icon for the applet \n");
                    println!("Example line .desktop file:");
                    println!("Exec=sh -c \"cosmic-panel-button --exec 'notify-send cosmic-panel-button_pressed' --name 'send-notification' --icon bell\"");
                    std::process::exit(1)
                }
                _ => {}
            }
        }

        desktop = Some(Desktop {
            name: name.unwrap_or_else(|| panic!("Name is a required argument")),
            icon: icon,
            exec: exec.unwrap_or_else(|| panic!("Exec is a required argument")),
        });
    } else {
        let arg = env::args()
            .nth(1)
            .expect("Requires a desktop id or --help for an argument.");

        let filename = format!("{arg}.desktop");

        for mut path in freedesktop_desktop_entry::default_paths() {
            path.push(&filename);
            if let Ok(bytes) = fs::read_to_string(&path) {
                if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                    desktop = Some(Desktop {
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
        desktop = Some(desktop.unwrap_or_else(|| {
            panic!("Failed to find valid desktop file '{filename}' in search paths")
        }));
    }
    cosmic::applet::run::<Button>(true, desktop.unwrap())
}
