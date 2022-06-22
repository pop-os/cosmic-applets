// TODO: use udev to monitor for brightness changes?
// How should key bindings be handled? Need something like gnome-settings-daemon?

use std::{
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
    str::FromStr,
};

#[zbus::dbus_proxy(
    default_service = "org.freedesktop.login1",
    interface = "org.freedesktop.login1.Session"
)]
trait LogindSession {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

struct Backlight(PathBuf);

impl Backlight {
    fn brightness(&self) -> Option<u32> {
        self.prop("brightness")
    }

    fn max_brightness(&self) -> Option<u32> {
        self.prop("max_brightness")
    }

    async fn set_brightness(
        &self,
        session: &LogindSessionProxy<'_>,
        value: u32,
    ) -> zbus::Result<()> {
        session.set_brightness("backlight", "", value).await // XXX
    }

    fn prop<T: FromStr>(&self, name: &str) -> Option<T> {
        let mut file = File::open(&self.0.join(name)).ok()?;
        let mut s = String::new();
        file.read_to_string(&mut s).ok()?;
        s.parse().ok()
    }
}

// Choose backlight with most "precision". This is what `light` does.
fn backlight() -> io::Result<Option<Backlight>> {
    let mut best_backlight = None;
    let mut best_max_brightness = 0;
    for i in fs::read_dir("/sys/class/backlight")? {
        let backlight = Backlight(i?.path());
        if let Some(max_brightness) = backlight.max_brightness() {
            if max_brightness > best_max_brightness {
                best_backlight = Some(backlight);
                best_max_brightness = max_brightness;
            }
        }
    }
    Ok(best_backlight)
}

// TODO: Cache device, max_brightness, etc.
async fn set_display_brightness(brightness: f64) -> io::Result<()> {
    if let Some(backlight) = backlight()? {
        if let Some(max_brightness) = backlight.max_brightness() {
            let value = brightness.clamp(0., 1.) * (max_brightness as f64);
            let value = value.round() as u32;
            // XXX TODO
            backlight.set_brightness(todo!(), value).await;
        }
    }
    Ok(())
}

// TODO: keyboard backlight
