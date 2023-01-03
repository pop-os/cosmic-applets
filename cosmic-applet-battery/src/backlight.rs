// TODO: use udev to monitor for brightness changes?
// How should key bindings be handled? Need something like gnome-settings-daemon?

use std::{
    fmt::Debug,
    fs::File,
    hash::Hash,
    io::{self, Read},
    os::unix::ffi::OsStrExt,
    path::Path,
    str::{self, FromStr},
};

use cosmic::iced;
use iced::subscription;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

const BACKLIGHT_SYSDIR: &str = "/sys/class/backlight";

#[zbus::dbus_proxy(
    default_service = "org.freedesktop.login1",
    interface = "org.freedesktop.login1.Session",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait LogindSession {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

#[derive(Clone)]
pub struct Backlight(String);

impl Backlight {
    pub async fn brightness(&self) -> Option<u32> {
        self.prop("brightness").await
    }

    // XXX cache value. Async?
    pub async fn max_brightness(&self) -> Option<u32> {
        self.prop("max_brightness").await
    }

    pub async fn set_brightness(
        &self,
        session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {
        session.set_brightness("backlight", &self.0, value).await
    }

    async fn prop<T: FromStr>(&self, name: &str) -> Option<T> {
        let path = Path::new(BACKLIGHT_SYSDIR).join(&self.0).join(name);
        let mut file = File::open(path).ok()?;
        let mut s = String::new();
        file.read_to_string(&mut s).ok()?;
        s.trim().parse().ok()
    }
}

// Choose backlight with most "precision". This is what `light` does.
pub async fn backlight() -> io::Result<Option<Backlight>> {
    let mut best_backlight = None;
    let mut best_max_brightness = 0;
    let mut dir_stream = tokio::fs::read_dir(BACKLIGHT_SYSDIR).await?;
    while let Ok(Some(entry)) = dir_stream.next_entry().await {
        if let Ok(filename) = str::from_utf8(entry.file_name().as_bytes()) {
            let backlight = Backlight(filename.to_string());
            if let Some(max_brightness) = backlight.max_brightness().await {
                if max_brightness > best_max_brightness {
                    best_backlight = Some(backlight);
                    best_max_brightness = max_brightness;
                }
            }
        }
    }
    Ok(best_backlight)
}

pub fn screen_backlight_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced::Subscription<(I, ScreenBacklightUpdate)> {
    subscription::unfold(id, State::Ready, move |state| start_listening(id, state))
}

pub enum State {
    Ready,
    Waiting(
        Backlight,
        LogindSessionProxy<'static>,
        UnboundedReceiver<ScreenBacklightRequest>,
    ),
    Finished,
}

async fn start_listening<I: Copy>(
    id: I,
    state: State,
) -> (Option<(I, ScreenBacklightUpdate)>, State) {
    match state {
        State::Ready => {
            let conn = match zbus::Connection::system().await {
                Ok(conn) => conn,
                Err(_) => return (None, State::Finished),
            };
            let screen_proxy = match LogindSessionProxy::builder(&conn).build().await {
                Ok(p) => p,
                Err(_) => return (None, State::Finished),
            };
            let backlight = match backlight().await {
                Ok(Some(b)) => b,
                _ => return (None, State::Finished),
            };
            let (tx, rx) = unbounded_channel();

            let b = (backlight.brightness().await.unwrap_or_default() as f64
                / backlight.max_brightness().await.unwrap_or(1) as f64)
                .clamp(0., 1.);
            return (
                Some((id, ScreenBacklightUpdate::Init(tx, b))),
                State::Waiting(backlight, screen_proxy, rx),
            );
        }
        State::Waiting(backlight, proxy, mut rx) => match rx.recv().await {
            Some(req) => match req {
                ScreenBacklightRequest::Get => {
                    let msg = if let Some(max_brightness) = backlight.max_brightness().await {
                        let value = (backlight.brightness().await.unwrap_or_default() as f64
                            / max_brightness as f64)
                            .clamp(0., 1.);
                        Some((id, ScreenBacklightUpdate::Update(value)))
                    } else {
                        None
                    };
                    (msg, State::Waiting(backlight, proxy, rx))
                }
                ScreenBacklightRequest::Set(value) => {
                    if let Some(max_brightness) = backlight.max_brightness().await {
                        let value = value.clamp(0., 1.) * (max_brightness as f64);
                        let value = value.round() as u32;
                        let _ = backlight.set_brightness(&proxy, value).await;
                    }
                    (None, State::Waiting(backlight, proxy, rx))
                }
            },
            None => (None, State::Finished),
        },
        State::Finished => iced::futures::future::pending().await,
    }
}

#[derive(Debug, Clone)]
pub enum ScreenBacklightUpdate {
    Update(f64),
    Init(UnboundedSender<ScreenBacklightRequest>, f64),
}

#[derive(Debug, Clone)]
pub enum ScreenBacklightRequest {
    Get,
    Set(f64),
}

/*
// TODO: Cache device, max_brightness, etc.
async fn set_display_brightness(brightness: f64) -> io::Result<()> {
    if let Some(backlight) = backlight()? {
        if let Some(max_brightness) = backlight.max_brightness() {
            let value = brightness.clamp(0., 1.) * (max_brightness as f64);
            let value = value.round() as u32;
            let connection = zbus::Connection::system().await?;
            if let Ok(session) = LogindSessionProxy::builder(&connection).build().await {
                backlight.set_brightness(&session, value).await;
            }
        }
    }
    Ok(())
}
*/

// TODO: keyboard backlight
