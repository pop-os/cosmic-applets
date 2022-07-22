use gtk4::prelude::*;
use libpulse_binding::volume::Volume;

use crate::{pa::DeviceInfo, volume_scale::VolumeScale};

pub fn update_volume(device: &DeviceInfo, scale: &VolumeScale) {
    scale.set_name(device.name.clone());
    scale.set_volume(&device.volume);
}
