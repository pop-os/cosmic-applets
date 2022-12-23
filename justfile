rootdir := ''
prefix := '/usr'
clean := '0'
debug := '0'
vendor := '0'
target := if debug == '1' { 'debug' } else { 'release' }
vendor_args := if vendor == '1' { '--frozen --offline' } else { '' }
debug_args := if debug == '1' { '' } else { '--release' }
cargo_args := vendor_args + ' ' + debug_args


sharedir := rootdir + prefix + '/share'
iconsdir := sharedir + '/icons/hicolor/scalable/apps'
bindir := rootdir + prefix + '/bin'

app_list_id := 'com.system76.CosmicAppList'
audio_id := 'com.system76.CosmicAppletAudio'
battery_id := 'com.system76.CosmicAppletBattery'
graphics_id := 'com.system76.CosmicAppletGraphics'
network_id := 'com.system76.CosmicAppletNetwork'
notifications_id := 'com.system76.CosmicAppletNotifications'
power_id := 'com.system76.CosmicAppletPower'
workspaces_id := 'com.system76.CosmicAppletWorkspaces'
status_area_id := 'com.system76.CosmicAppletStatusArea'
time_id := 'com.system76.CosmicAppletTime'
app_button_id := 'com.system76.CosmicPanelAppButton'
workspaces_button_id := 'com.system76.CosmicPanelWorkspacesButton'

build: _extract_vendor
    #!/usr/bin/env bash
    cargo build {{cargo_args}}


# Installs files into the system
install:
    # audio
    install -Dm0644 cosmic-applet-audio/data/icons/{{audio_id}}.svg {{iconsdir}}/{{audio_id}}.svg
    install -Dm0644 cosmic-applet-audio/data/{{audio_id}}.desktop {{sharedir}}/applications/{{audio_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-audio {{bindir}}/cosmic-applet-audio

    # app list
    install -Dm0644 cosmic-app-list/data/icons/{{app_list_id}}-symbolic.svg {{iconsdir}}/{{app_list_id}}-symbolic.svg
    install -Dm0644 cosmic-app-list/data/icons/{{app_list_id}}.Devel.svg {{iconsdir}}/{{app_list_id}}.Devel.svg
    install -Dm0644 cosmic-app-list/data/icons/{{app_list_id}}.svg {{iconsdir}}/{{app_list_id}}.svg
    install -Dm0644 cosmic-app-list/data/{{app_list_id}}.desktop {{sharedir}}/applications/{{app_list_id}}.desktop
    install -Dm0755 target/release/cosmic-app-list {{bindir}}/cosmic-app-list

    # network
    install -Dm0644 cosmic-applet-network/data/icons/{{network_id}}.svg {{iconsdir}}/{{network_id}}.svg
    install -Dm0644 cosmic-applet-network/data/{{network_id}}.desktop {{sharedir}}/applications/{{network_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-network {{bindir}}/cosmic-applet-network

    # notifications
    install -Dm0644 cosmic-applet-notifications/data/icons/{{notifications_id}}.svg {{iconsdir}}/{{notifications_id}}.svg
    install -Dm0644 cosmic-applet-notifications/data/{{notifications_id}}.desktop {{sharedir}}/applications/{{notifications_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-notifications {{bindir}}/cosmic-applet-notifications

    # power
    install -Dm0644 cosmic-applet-power/data/icons/{{power_id}}.svg {{iconsdir}}/{{power_id}}.svg
    install -Dm0644 cosmic-applet-power/data/{{power_id}}.desktop {{sharedir}}/applications/{{power_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-power {{bindir}}/cosmic-applet-power

    # time
    install -Dm0644 cosmic-applet-time/data/icons/{{time_id}}.svg {{iconsdir}}/{{time_id}}.svg
    install -Dm0644 cosmic-applet-time/data/{{time_id}}.desktop {{sharedir}}/applications/{{time_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-time {{bindir}}/cosmic-applet-time

    # app library button
    install -Dm0644 cosmic-panel-app-button/data/icons/{{app_button_id}}.svg {{iconsdir}}/{{app_button_id}}.svg
    install -Dm0644 cosmic-panel-app-button/data/{{app_button_id}}.desktop {{sharedir}}/applications/{{app_button_id}}.desktop

    # workspaces button
    install -Dm0644 cosmic-panel-workspaces-button/data/icons/{{workspaces_button_id}}.svg {{iconsdir}}/{{workspaces_button_id}}.svg
    install -Dm0644 cosmic-panel-workspaces-button/data/{{workspaces_button_id}}.desktop {{sharedir}}/applications/{{workspaces_button_id}}.desktop

    # graphics
    install -Dm0644 cosmic-applet-graphics/data/icons/{{graphics_id}}.svg {{iconsdir}}/{{graphics_id}}.svg
    install -Dm0644 cosmic-applet-graphics/data/{{graphics_id}}.desktop {{sharedir}}/applications/{{graphics_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-graphics {{bindir}}/cosmic-applet-graphics

    # workspaces
    install -Dm0644 cosmic-applet-workspaces/data/icons/{{workspaces_id}}.svg {{iconsdir}}/{{workspaces_id}}.svg
    install -Dm0644 cosmic-applet-workspaces/data/{{workspaces_id}}.desktop {{sharedir}}/applications/{{workspaces_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-workspaces {{bindir}}/cosmic-applet-workspaces

    # battery
    install -Dm0644 cosmic-applet-battery/data/icons/{{battery_id}}.svg {{iconsdir}}/{{battery_id}}.svg
    install -Dm0644 cosmic-applet-battery/data/{{battery_id}}.desktop {{sharedir}}/applications/{{battery_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-battery {{bindir}}/cosmic-applet-battery

# Extracts vendored dependencies if vendor=1
_extract_vendor:
    #!/usr/bin/env sh
    if test {{vendor}} = 1; then
        rm -rf vendor; tar pxf vendor.tar
    fi
