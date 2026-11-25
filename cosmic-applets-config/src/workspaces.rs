// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic_config::{CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, CosmicConfigEntry, Default)]
#[version = 1]
pub struct WorkspacesAppletConfig {
    pub use_custom_glyphs: bool,
    pub use_colored_buttons: bool,
    pub active_workspace_glyph: char,
    pub default_workspace_glyph: char,
    pub urgent_workspace_glyph: char,
}
