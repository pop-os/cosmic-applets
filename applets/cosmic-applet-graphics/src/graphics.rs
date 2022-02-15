// SPDX-License-Identifier: LGPL-3.0-or-later
use crate::dbus::PowerDaemonProxy;
use zbus::Result;

pub enum Graphics {
    Integrated,
    Hybrid,
    External,
    Compute,
}

pub async fn get_current_graphics(daemon: &PowerDaemonProxy<'_>) -> Result<Graphics> {
    let graphics = daemon.get_graphics().await?;
    match graphics.as_str() {
        "integrated" => Ok(Graphics::Integrated),
        "hybrid" => Ok(Graphics::Hybrid),
        "external" => Ok(Graphics::External),
        "compute" => Ok(Graphics::Compute),
        _ => panic!("Unknown graphics profile: {}", graphics),
    }
}

pub async fn get_default_graphics(daemon: &PowerDaemonProxy<'_>) -> Result<Graphics> {
    let graphics = daemon.get_default_graphics().await?;
    match graphics.as_str() {
        "integrated" => Ok(Graphics::Integrated),
        "hybrid" => Ok(Graphics::Hybrid),
        "external" => Ok(Graphics::External),
        "compute" => Ok(Graphics::Compute),
        _ => panic!("Unknown graphics profile: {}", graphics),
    }
}

pub async fn set_graphics(daemon: &PowerDaemonProxy<'_>, graphics: Graphics) -> Result<()> {
    let graphics_str = match graphics {
        Graphics::Integrated => "integrated",
        Graphics::Hybrid => "hybrid",
        Graphics::External => "external",
        Graphics::Compute => "compute",
    };
    daemon.set_graphics(graphics_str).await
}
