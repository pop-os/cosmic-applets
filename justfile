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
    pushd applets/cosmic-app-list/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-audio/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-network/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-graphics/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-battery/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-power/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-time/
    cargo build {{cargo_args}}
    popd
    pushd applets/cosmic-applet-workspaces/
    cargo build {{cargo_args}}
    popd
    cargo build {{cargo_args}}

# Installs files into the system
install:
    # audio
    install -Dm0644 applets/cosmic-applet-audio/data/icons/{{audio_id}}.svg {{iconsdir}}/{{audio_id}}.svg
    install -Dm0644 applets/cosmic-applet-audio/data/{{audio_id}}.desktop {{sharedir}}/applications/{{audio_id}}.desktop
    install -Dm0755 applets/cosmic-applet-audio/target/release/cosmic-applet-audio {{bindir}}/cosmic-applet-audio

    # app list
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}-symbolic.svg {{iconsdir}}/{{app_list_id}}-symbolic.svg
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}.Devel.svg {{iconsdir}}/{{app_list_id}}.Devel.svg
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}.svg {{iconsdir}}/{{app_list_id}}.svg
    install -Dm0644 applets/cosmic-app-list/data/{{app_list_id}}.desktop {{sharedir}}/applications/{{app_list_id}}.desktop
    install -Dm0755 applets/cosmic-app-list/target/release/cosmic-app-list {{bindir}}/cosmic-app-list

    # network
    install -Dm0644 applets/cosmic-applet-network/data/icons/{{network_id}}.svg {{iconsdir}}/{{network_id}}.svg
    install -Dm0644 applets/cosmic-applet-network/data/{{network_id}}.desktop {{sharedir}}/applications/{{network_id}}.desktop
    install -Dm0755 applets/cosmic-applet-network/target/release/cosmic-applet-network {{bindir}}/cosmic-applet-network

    # notifications
    install -Dm0644 applets/cosmic-applet-notifications/data/icons/{{notifications_id}}.svg {{iconsdir}}/{{notifications_id}}.svg
    install -Dm0644 applets/cosmic-applet-notifications/data/{{notifications_id}}.desktop {{sharedir}}/applications/{{notifications_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-notifications {{bindir}}/cosmic-applet-notifications

    # power
    install -Dm0644 applets/cosmic-applet-power/data/icons/{{power_id}}.svg {{iconsdir}}/{{power_id}}.svg
    install -Dm0644 applets/cosmic-applet-power/data/{{power_id}}.desktop {{sharedir}}/applications/{{power_id}}.desktop
    install -Dm0755 applets/cosmic-applet-power/target/release/cosmic-applet-power {{bindir}}/cosmic-applet-power

    # status area
    install -Dm0644 applets/cosmic-applet-status-area/data/icons/{{status_area_id}}.svg {{iconsdir}}/{{status_area_id}}.svg
    install -Dm0644 applets/cosmic-applet-status-area/data/{{status_area_id}}.desktop {{sharedir}}/applications/{{status_area_id}}.desktop
    install -Dm0755 target/release/cosmic-applet-status-area {{bindir}}/cosmic-applet-status-area

    # time
    install -Dm0644 applets/cosmic-applet-time/data/icons/{{time_id}}.svg {{iconsdir}}/{{time_id}}.svg
    install -Dm0644 applets/cosmic-applet-time/data/{{time_id}}.desktop {{sharedir}}/applications/{{time_id}}.desktop
    install -Dm0755 applets/cosmic-applet-time/target/release/cosmic-applet-time {{bindir}}/cosmic-applet-time

    # app library button
    install -Dm0644 applets/cosmic-panel-app-button/data/icons/{{app_button_id}}.svg {{iconsdir}}/{{app_button_id}}.svg
    install -Dm0644 applets/cosmic-panel-app-button/data/{{app_button_id}}.desktop {{sharedir}}/applications/{{app_button_id}}.desktop

    # workspaces button
    install -Dm0644 applets/cosmic-panel-workspaces-button/data/icons/{{workspaces_button_id}}.svg {{iconsdir}}/{{workspaces_button_id}}.svg
    install -Dm0644 applets/cosmic-panel-workspaces-button/data/{{workspaces_button_id}}.desktop {{sharedir}}/applications/{{workspaces_button_id}}.desktop

    # panel button
    install -Dm0755 target/release/cosmic-panel-button {{bindir}}/cosmic-panel-button

    # graphics
    install -Dm0644 applets/cosmic-applet-graphics/data/icons/{{graphics_id}}.svg {{iconsdir}}/{{graphics_id}}.svg
    install -Dm0644 applets/cosmic-applet-graphics/data/{{graphics_id}}.desktop {{sharedir}}/applications/{{graphics_id}}.desktop
    install -Dm0755 applets/cosmic-applet-graphics/target/release/cosmic-applet-graphics {{bindir}}/cosmic-applet-graphics

    # workspaces
    install -Dm0644 applets/cosmic-applet-workspaces/data/icons/{{workspaces_id}}.svg {{iconsdir}}/{{workspaces_id}}.svg
    install -Dm0644 applets/cosmic-applet-workspaces/data/{{workspaces_id}}.desktop {{sharedir}}/applications/{{workspaces_id}}.desktop
    install -Dm0755 applets/cosmic-applet-workspaces/target/release/cosmic-applet-workspaces {{bindir}}/cosmic-applet-workspaces

    # battery
    install -Dm0644 applets/cosmic-applet-battery/data/icons/{{battery_id}}.svg {{iconsdir}}/{{battery_id}}.svg
    install -Dm0644 applets/cosmic-applet-battery/data/{{battery_id}}.desktop {{sharedir}}/applications/{{battery_id}}.desktop
    install -Dm0755 applets/cosmic-applet-battery/target/release/cosmic-applet-battery {{bindir}}/cosmic-applet-battery

# Extracts vendored dependencies if vendor=1
_extract_vendor:
    #!/usr/bin/env sh
    if test {{vendor}} = 1; then
        rm -rf vendor; tar pxf vendor.tar
        rm -rf applets/cosmic-applet-graphics/vendor; tar xf applets/cosmic-applet-graphics/vendor.tar --directory applets/cosmic-applet-graphics
        rm -rf applets/cosmic-applet-workspaces/vendor; tar xf applets/cosmic-applet-workspaces/vendor.tar --directory applets/cosmic-applet-workspaces
        rm -rf applets/cosmic-applet-battery/vendor; tar xf applets/cosmic-applet-battery/vendor.tar --directory applets/cosmic-applet-battery
        rm -rf applets/cosmic-applet-audio/vendor; tar xf applets/cosmic-applet-audio/vendor.tar --directory applets/cosmic-applet-audio
        rm -rf applets/cosmic-applet-power/vendor; tar xf applets/cosmic-applet-power/vendor.tar --directory applets/cosmic-applet-power
        rm -rf applets/cosmic-applet-time/vendor; tar xf applets/cosmic-applet-time/vendor.tar --directory applets/cosmic-applet-time
        rm -rf applets/cosmic-applet-network/vendor; tar xf applets/cosmic-applet-network/vendor.tar --directory applets/cosmic-applet-network
        rm -rf applets/cosmic-app-list/vendor; tar xf applets/cosmic-app-list/vendor.tar --directory applets/cosmic-app-list
    fi
