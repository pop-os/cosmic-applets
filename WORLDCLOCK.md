# World clock applet

A fork of System76's `cosmic-applet-time` that adds extra time zones and an hour
stepper to the calendar popup, so you can answer "when it is 3pm here, what time
is it there" without leaving the panel.

Based on cosmic-applets `b933e53`, which matches the `cosmic-applets` 1.0.15 package
on Pop!_OS 24.04.

## What the popup shows

```
[ cog ]                                  [ reset ] [ - ] [ + ]
  Sao Paulo (local)                                   10:32 AM
  Toronto                                              9:32 AM
  Chicago                                              8:32 AM
  Pacific                                              6:32 AM
-------------------------------------------------------------
September 21, 2026                                 [ < ] [ > ]
        ... calendar ...
-------------------------------------------------------------
  Date, Time and Calendar Settings...
```

Every row is projected from one instant, so the times stay right across daylight
saving changes that only some of the zones observe. The stepper lands on whole
hours: from 10:35, one press up reads 11:00 and one press down reads 10:00.
Pick a calendar day and the stepper applies to that day. A row shows `+1` or `-1`
when that zone is on a different date.

The cog opens the zone list in `cosmic-edit`. Reset appears once the popup is
describing anything other than the current time.

## Install

Needs Pop!_OS 24.04 or newer running COSMIC (glibc 2.39 and `libxkbcommon0`).

```sh
cargo build --release -p cosmic-applet-time
./install.sh
```

Then add `io.github.renanstigliani.CosmicAppletWorldClock` to the applet list in
`~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings` and run
`pkill -x cosmic-panel`.

To install without building, copy a release binary to
`~/.local/bin/cosmic-applet-worldclock` and the desktop file from
`cosmic-applet-time/data/` to `~/.local/share/applications/`. The binary links
only against libc, libgcc, libm and libxkbcommon.

## Build requirements

Rust 1.93 (the repo pins it in `rust-toolchain.toml`) and:

```sh
sudo apt install -y libxkbcommon-dev libdrm-dev libclang-dev \
  libegl-dev libgbm-dev libgl-dev
```

The first build writes about 1.8 GB to `target/`.

## Configuration

Settings live in `~/.config/cosmic/io.github.renanstigliani.CosmicAppletWorldClock/v1/`,
one file per setting. Everything except `world_clocks` is standard
`cosmic-applet-time` configuration.

`world_clocks` holds the extra zones:

```
["SF|America/Los_Angeles", "Europe/Amsterdam", "UTC"]
```

An entry is an IANA time zone id. Put a label and a `|` in front to choose the text
shown in the row; without one, the city part of the id is used. Entries naming an
unknown zone are skipped. The applet picks up edits right away.

## What changed from System76's code

- `cosmic-applets-config/src/time.rs`: the `world_clocks` field.
- `cosmic-applet-time/src/window.rs`: the zone rows, the stepper, the cog, and a
  separate `APP_ID` so this applet keeps its own configuration and can run next to
  the stock clock.

## Licence

GPL-3.0-only, the same as the code it is built on.
