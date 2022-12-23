use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

fn get_default_color(names: &[&str], is_dark: bool) -> HashMap<String, [f64; 4]> {
    let css = if is_dark {
        adw_user_colors_lib::colors::ColorOverrides::dark_default().as_css()
    } else {
        adw_user_colors_lib::colors::ColorOverrides::light_default().as_css()
    };
    names
        .iter()
        .filter_map(|name| {
            let window_bg_color_pattern = &format!("@define-color {name}");
            css.rfind(window_bg_color_pattern)
                .and_then(|i| css.get(i + window_bg_color_pattern.len()..))
                .and_then(|color_str| {
                    csscolorparser::parse(&color_str.trim().replace(";", "")).ok()
                })
                .map(|c| (name.to_string(), c.to_array()))
        })
        .collect()
}

fn get_colors(names: &[&str], path: &PathBuf) -> HashMap<String, [f64; 4]> {
    let file = match File::open(path) {
        Ok(f) => f,
        _ => return Default::default(),
    };

    BufReader::new(file)
        .lines()
        .filter_map(|l| l.ok())
        .filter_map(|line| {
            names.iter().find_map(|name| {
                line.rfind(&format!("@define-color {name}"))
                    .map(|i| (name, i))
                    .and_then(|(name, i)| {
                        line.get(i + format!("@define-color {name}").len()..)
                            .map(|s| (name, s))
                            .and_then(|(name, color_str)| {
                                csscolorparser::parse(&color_str.trim().replace(";", ""))
                                    .ok()
                                    .map(|c| (name.to_string(), c.to_array()))
                            })
                    })
            })
        })
        .collect()
}
