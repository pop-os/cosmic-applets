// TODO: use udev to monitor for brightness changes?
// How should key bindings be handled? Need something like gnome-settings-daemon?

use std::{
    fs::{self, File},
    io::{self, Read},
    os::unix::ffi::OsStrExt,
    path::Path,
    str::{self, FromStr},
};

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
    pub fn brightness(&self) -> Option<u32> {
        self.prop("brightness")
    }

    // XXX cache value. Async?
    pub fn max_brightness(&self) -> Option<u32> {
        self.prop("max_brightness")
    }

    pub async fn set_brightness(
        &self,
        session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {
        session.set_brightness("backlight", &self.0, value).await
    }

    fn prop<T: FromStr>(&self, name: &str) -> Option<T> {
        let path = Path::new(BACKLIGHT_SYSDIR).join(&self.0).join(name);
        let mut file = File::open(path).ok()?;
        let mut s = String::new();
        file.read_to_string(&mut s).ok()?;
        s.trim().parse().ok()
    }
}

// Choose backlight with most "precision". This is what `light` does.
pub fn backlight() -> io::Result<Option<Backlight>> {
    let mut best_backlight = None;
    let mut best_max_brightness = 0;
    for i in fs::read_dir(BACKLIGHT_SYSDIR)? {
        if let Ok(filename) = str::from_utf8(i?.file_name().as_bytes()) {
            let backlight = Backlight(filename.to_string());
            if let Some(max_brightness) = backlight.max_brightness() {
                if max_brightness > best_max_brightness {
                    best_backlight = Some(backlight);
                    best_max_brightness = max_brightness;
                }
            }
        }
    }
    Ok(best_backlight)
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
