// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use zbus::proxy;

#[proxy(
    interface = "com.system76.CosmicSession",
    default_service = "com.system76.CosmicSession",
    default_path = "/com/system76/CosmicSession"
)]
trait CosmicSession {
    fn exit(&self) -> zbus::Result<()>;
}
