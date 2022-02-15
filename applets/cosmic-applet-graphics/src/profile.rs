use crate::dbus::PowerDaemonProxy;
use zbus::Result;

pub enum Profile {
    Performance,
    Balanced,
    Battery,
}

pub async fn get_current_profile(daemon: &PowerDaemonProxy<'_>) -> Result<Profile> {
    let profile = daemon.get_profile().await?;
    match profile.as_str() {
        "Performance" => Ok(Profile::Performance),
        "Balanced" => Ok(Profile::Balanced),
        "Battery" => Ok(Profile::Battery),
        _ => panic!("Unknown profile: {}", profile),
    }
}

pub async fn set_profile(daemon: &PowerDaemonProxy<'_>, profile: Profile) -> zbus::Result<()> {
    match profile {
        Profile::Performance => daemon.performance().await,
        Profile::Balanced => daemon.balanced().await,
        Profile::Battery => daemon.battery().await,
    }
}
