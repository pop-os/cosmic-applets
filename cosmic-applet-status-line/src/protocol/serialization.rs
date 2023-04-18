// This implementation may be stricter in parsing than swaybar or i3bar. If this is an issue, the
// status command should probably be the one that's corrected to conform.

use cosmic::iced;
use serde::de::{Deserialize, Error};

fn sigcont() -> u8 {
    18
}

fn sigstop() -> u8 {
    19
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Header {
    pub version: u8,
    #[serde(default)]
    pub click_events: bool,
    #[serde(default = "sigcont")]
    pub cont_signal: u8,
    #[serde(default = "sigstop")]
    pub stop_signal: u8,
}

fn default_border() -> u32 {
    1
}

fn default_seperator_block_width() -> u32 {
    9
}

/// Deserialize string with RGB or RGBA color into `iced::Color`
fn deserialize_color<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<iced::Color>, D::Error> {
    let s = String::deserialize(deserializer)?;

    let unexpected_err = || {
        D::Error::invalid_value(
            serde::de::Unexpected::Str(&s),
            &"a color string #RRGGBBAA or #RRGGBB",
        )
    };

    // Must be 8 or 9 character string starting with #
    if !s.starts_with("#") || (s.len() != 7 && s.len() != 9) {
        return Err(unexpected_err());
    }

    let parse_hex = |component| u8::from_str_radix(component, 16).map_err(|_| unexpected_err());
    let r = parse_hex(&s[1..3])?;
    let g = parse_hex(&s[3..5])?;
    let b = parse_hex(&s[5..7])?;
    let a = if s.len() == 9 {
        parse_hex(&s[7..])? as f32 / 1.0
    } else {
        1.0
    };
    Ok(Some(iced::Color::from_rgba8(r, g, b, a)))
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum MinWidth {
    Int(u32),
    Str(String),
}

impl Default for MinWidth {
    fn default() -> Self {
        Self::Int(0)
    }
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Markup {
    #[default]
    None,
    Pango,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Block {
    pub full_text: String,
    pub short_text: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_color")]
    pub color: Option<iced::Color>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_color")]
    pub background: Option<iced::Color>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_color")]
    pub border: Option<iced::Color>,
    #[serde(default = "default_border")]
    pub border_top: u32,
    #[serde(default = "default_border")]
    pub border_bottom: u32,
    #[serde(default = "default_border")]
    pub border_left: u32,
    #[serde(default = "default_border")]
    pub border_right: u32,
    #[serde(default)]
    pub min_width: MinWidth,
    #[serde(default)]
    pub align: Align,
    pub name: Option<String>,
    pub instance: Option<String>,
    #[serde(default)]
    pub urgent: bool,
    #[serde(default)]
    pub separator: bool,
    #[serde(default = "default_seperator_block_width")]
    pub separator_block_width: u32,
    pub markup: Markup,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ClickEvent {
    pub name: Option<String>,
    pub instance: Option<String>,
    pub x: u32,
    pub y: u32,
    pub button: u32,
    pub event: u32,
    pub relative_x: u32,
    pub relative_y: u32,
    pub width: u32,
    pub height: u32,
}
