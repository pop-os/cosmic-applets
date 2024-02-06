// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::path::Path;
use std::process::Stdio;

/// Plays an audio file.
pub fn play(path: &Path) {
    let _result = tokio::process::Command::new("pw-play")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg(path)
        .spawn();
}

pub fn play_audio_volume_change() {
    play(Path::new(
        "/usr/share/sounds/freedesktop/stereo/audio-volume-change.oga",
    ));
}
