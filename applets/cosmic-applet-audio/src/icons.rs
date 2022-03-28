use std::collections::HashMap;

use freedesktop_desktop_entry::{default_paths, DesktopEntry, Iter};

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum DesktopApplication {
    Name(String),
    Binary(String),
}

pub fn parse_desktop_icons() -> HashMap<DesktopApplication, String> {
    let mut out = HashMap::new();
    for path in Iter::new(default_paths()) {
        let file = match std::fs::read_to_string(&path) {
            Ok(data) => data,
            _ => continue,
        };
        let entry = match DesktopEntry::decode(&path, &file) {
            Ok(entry) => entry,
            _ => continue,
        };
        let icon = match entry.icon() {
            Some(icon) => icon,
            None => continue,
        };
        if let Some(name) = entry.name(None) {
            out.insert(DesktopApplication::Name(name.into_owned()), icon.to_owned());
        };
        if let Some(exec) = entry
            .exec()
            .and_then(|entry| entry.split_whitespace().next())
        {
            out.insert(DesktopApplication::Binary(exec.to_owned()), icon.to_owned());
        };
    }
    out
}
