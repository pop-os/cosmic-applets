use cosmic::{
    applet::CosmicAppletHelper,
    iced::{
        self,
        wayland::InitialSurface,
        Application,
    },
    iced_sctk::layout::Limits,
    iced_style::application,
    iced_native::window,
};
use freedesktop_desktop_entry::DesktopEntry;
use std::{env, fs, process::Command};

#[derive(Clone, Default)]
struct Desktop {
    name: String,
    icon: Option<String>,
    exec: String,
}

struct Button {
    desktop: Desktop,
}

#[derive(Debug, Clone)]
enum Msg {
    Press,
}

impl iced::Application for Button {
    type Message = Msg;
    type Theme = cosmic::Theme;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Desktop;

    fn new(desktop: Desktop) -> (Self, iced::Command<Msg>) {
        (Button { desktop }, iced::Command::none())
    }

    fn title(&self) -> String {
        String::from("Button")
    }

    fn close_requested(&self, _id: window::Id) -> Msg {
        unimplemented!()
    }

    fn style(&self) -> <Self::Theme as application::StyleSheet>::Style {
        <Self::Theme as application::StyleSheet>::Style::Custom(|theme| application::Appearance {
            background_color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            text_color: theme.cosmic().on_bg_color().into(),
        })
    }

    fn subscription(&self) -> iced::Subscription<Msg> {
        iced::Subscription::none()
    }

    fn update(&mut self, message: Msg) -> iced::Command<Msg> {
        match message {
            Msg::Press => {
                let _ = Command::new("sh").arg("-c").arg(&self.desktop.exec).spawn();
                iced::Command::none()
            }
        }
    }

    fn view(&self, _id: window::Id) -> cosmic::Element<Msg> {
        // TODO icon?
        cosmic::widget::button(cosmic::theme::Button::Text)
            .text(&self.desktop.name)
            .on_press(Msg::Press)
            .into()
    }
}

pub fn main() -> iced::Result {
    let id = env::args()
        .skip(1)
        .next()
        .expect("Requires desktop file id as argument.");

    let filename = format!("{id}.desktop");
    let mut desktop = None;
    for mut path in freedesktop_desktop_entry::default_paths() {
        path.push(&filename);
        if let Ok(bytes) = fs::read_to_string(&path) {
            if let Ok(entry) = DesktopEntry::decode(&path, &bytes) {
                desktop = Some(Desktop {
                    name: entry
                        .name(None)
                        .map(|x| x.to_string())
                        .expect(&format!("Desktop file '{filename}' doesn't have `Name`")),
                    icon: entry.icon().map(|x| x.to_string()),
                    exec: entry
                        .exec()
                        .map(|x| x.to_string())
                        .expect(&format!("Desktop file '{filename}' doesn't have `Exec`")),
                });
                break;
            }
        }
    }
    let desktop = desktop.expect(&format!(
        "Failed to find valid desktop file '{filename}' in search paths"
    ));
    let helper = CosmicAppletHelper::default();
    let mut settings = iced::Settings {
        flags: desktop,
        ..helper.window_settings()
    };
    match &mut settings.initial_surface {
        InitialSurface::XdgWindow(s) => {
            s.iced_settings.min_size = Some((1, 1));
            s.iced_settings.max_size = None;
            s.autosize = true;
            s.size_limits = Limits::NONE.min_height(1).min_width(1);
        }
        _ => unreachable!(),
    };
    Button::run(settings)
}
