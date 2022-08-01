// SPDX-License-Identifier: GPL-3.0-or-later
use zbus::dbus_proxy;

#[dbus_proxy(
    interface = "com.system76.CosmicSession",
    default_service = "com.system76.CosmicSession",
    default_path = "/com/system76/CosmicSession"
)]
trait CosmicSession {
    fn exit(&self) -> zbus::Result<()>;
}
