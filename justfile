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
graphics_id := 'com.system76.CosmicAppletGraphics'
network_id := 'com.system76.CosmicAppletNetwork'
power_id := 'com.system76.CosmicAppletPower'
status_area_id := 'com.system76.CosmicAppletStatusArea'
app_button_id := 'com.system76.CosmicPanelAppButton'
workspaces_button_id := 'com.system76.CosmicPanelWorkspacesButton'

all: _extract_vendor
    cargo build {{cargo_args}}

# Installs files into the system
install:
    # app list
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}-symbolic.svg {{iconsdir}}/{{app_list_id}}-symbolic.svg
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}.Devel.svg {{iconsdir}}/{{app_list_id}}.Devel.svg
    install -Dm0644 applets/cosmic-app-list/data/icons/{{app_list_id}}.svg {{iconsdir}}/{{app_list_id}}.svg
    install -Dm0644 applets/cosmic-app-list/data/{{app_list_id}}.desktop {{sharedir}}/applications/{{app_list_id}}.desktop
    install -Dm04755 target/release/cosmic-app-list {{bindir}}/cosmic-app-list

    # audio
    install -Dm0644 applets/cosmic-applet-audio/data/icons/{{audio_id}}.svg {{iconsdir}}/{{audio_id}}.svg
    install -Dm0644 applets/cosmic-applet-audio/data/{{audio_id}}.desktop {{sharedir}}/applications/{{audio_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-audio {{bindir}}/cosmic-applet-audio

    # graphics
    install -Dm0644 applets/cosmic-applet-graphics/data/icons/{{graphics_id}}.svg {{iconsdir}}/{{graphics_id}}.svg
    install -Dm0644 applets/cosmic-applet-graphics/data/{{graphics_id}}.desktop {{sharedir}}/applications/{{graphics_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-graphics {{bindir}}/cosmic-applet-graphics

    # network
    install -Dm0644 applets/cosmic-applet-network/data/icons/{{network_id}}.svg {{iconsdir}}/{{network_id}}.svg
    install -Dm0644 applets/cosmic-applet-network/data/{{network_id}}.desktop {{sharedir}}/applications/{{network_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-network {{bindir}}/cosmic-applet-network

    # power
    install -Dm0644 applets/cosmic-applet-power/data/icons/{{power_id}}.svg {{iconsdir}}/{{power_id}}.svg
    install -Dm0644 applets/cosmic-applet-power/data/{{power_id}}.desktop {{sharedir}}/applications/{{power_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-power {{bindir}}/cosmic-applet-power

    # status area
    install -Dm0644 applets/cosmic-applet-status-area/data/icons/{{status_area_id}}.svg {{iconsdir}}/{{status_area_id}}.svg
    install -Dm0644 applets/cosmic-applet-status-area/data/{{status_area_id}}.desktop {{sharedir}}/applications/{{status_area_id}}.desktop
    install -Dm04755 target/release/cosmic-applet-status-area {{bindir}}/cosmic-applet-status-area

    # app library button
    install -Dm0644 applets/cosmic-panel-app-button/data/icons/{{app_button_id}}.svg {{iconsdir}}/{{app_button_id}}.svg
    install -Dm0644 applets/cosmic-panel-app-button/data/{{app_button_id}}.desktop {{sharedir}}/applications/{{app_button_id}}.desktop

    # workspaces button
    install -Dm0644 applets/cosmic-panel-workspaces-button/data/icons/{{workspaces_button_id}}.svg {{iconsdir}}/{{workspaces_button_id}}.svg
    install -Dm0644 applets/cosmic-panel-workspaces-button/data/{{workspaces_button_id}}.desktop {{sharedir}}/applications/{{workspaces_button_id}}.desktop

    # panel button
    install -Dm04755 target/release/cosmic-panel-button {{bindir}}/cosmic-panel-button

# Extracts vendored dependencies if vendor=1
_extract_vendor:
    #!/usr/bin/env sh
    if test {{vendor}} = 1; then
        rm -rf vendor; tar pxf vendor.tar
    fi
