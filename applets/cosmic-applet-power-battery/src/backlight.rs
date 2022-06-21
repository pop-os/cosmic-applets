use std::{
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
    str::FromStr,
};

struct Backlight(PathBuf);

impl Backlight {
    fn brightness(&self) -> Option<i64> {
        self.prop("brightness")
    }

    fn max_brightness(&self) -> Option<i64> {
        self.prop("max_brightness")
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
