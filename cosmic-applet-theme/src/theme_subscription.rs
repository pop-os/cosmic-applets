// Copyright 2025 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::{Subscription, futures::SinkExt, stream};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ThemeUpdate {
    Changed(bool), // is_dark
}

pub fn theme_subscription(theme_file: PathBuf) -> Subscription<ThemeUpdate> {
    Subscription::run_with_id(
        "theme-file-watch",
        stream::channel(1, move |mut output| async move {
            let (tx, mut rx) = mpsc::channel(20);

            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if matches!(event.kind, notify::EventKind::Modify(_)) {
                            let _ = tx.try_send(());
                        }
                    }
                },
                Config::default(),
            ) {
                Ok(watcher) => watcher,
                Err(e) => {
                    tracing::error!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&theme_file, RecursiveMode::NonRecursive) {
                tracing::error!("Failed to watch theme file: {}", e);
                return;
            }

            // Send initial state
            if let Ok(content) = std::fs::read_to_string(&theme_file) {
                let is_dark = content.trim() == "true";
                tracing::debug!("Theme subscription: initial state is_dark={}", is_dark);
                let _ = output.send(ThemeUpdate::Changed(is_dark)).await;
            } else {
                tracing::warn!("Theme subscription: failed to read initial theme file");
            }

            while let Some(_) = rx.recv().await {
                if let Ok(content) = std::fs::read_to_string(&theme_file) {
                    let is_dark = content.trim() == "true";
                    tracing::debug!("Theme subscription: detected change, is_dark={}", is_dark);
                    let _ = output.send(ThemeUpdate::Changed(is_dark)).await;
                } else {
                    tracing::warn!("Theme subscription: failed to read theme file on change");
                }
            }
        }),
    )
}
