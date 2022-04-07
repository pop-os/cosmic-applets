use gtk4::{prelude::*, Scale};
use libpulse_binding::volume::Volume;
use pulsectl::controllers::types::DeviceInfo;

pub fn update_volume(device: &DeviceInfo, scale: &Scale) {
    scale.set_value((device.volume.avg().0 as f64 / Volume::NORMAL.0 as f64) * 100.);
}
