use std::str::FromStr;
use toml_edit::{Document, Table, Array, TomlError};

struct Buttons<'a>(&'a Table);

struct Config(Document);

impl Config {
    fn new(s: &str) -> Result<Self, TomlError> {
        Ok(Self(Document::from_str(s)?))
    }

    fn buttons(&self) -> Option<Buttons> {
        Some(Buttons(self.0.as_table().get("buttons")?.as_table()?))
    }
}
