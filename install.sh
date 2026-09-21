#!/usr/bin/env sh
# Installs the world clock applet for the current user.
#
# Build it first:
#   cargo build --release -p cosmic-applet-time
set -eu

APP_ID=io.github.renanstigliani.CosmicAppletWorldClock
BIN=cosmic-applet-worldclock
SRC=$(cd "$(dirname "$0")" && pwd)

if [ ! -x "$SRC/target/release/cosmic-applet-time" ]; then
    echo "Build it first: cargo build --release -p cosmic-applet-time" >&2
    exit 1
fi

install -Dm0755 "$SRC/target/release/cosmic-applet-time" \
    "$HOME/.local/bin/$BIN"
install -Dm0644 "$SRC/cosmic-applet-time/data/$APP_ID.desktop" \
    "$HOME/.local/share/applications/$APP_ID.desktop"

case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "Note: $HOME/.local/bin is not on PATH. Add it in ~/.profile." >&2 ;;
esac

echo "Installed. Add \"$APP_ID\" to the applet list in one of:"
echo "  ~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings"
echo "  ~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_center"
echo "then run: pkill -x cosmic-panel"
