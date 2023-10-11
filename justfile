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
iconsdir := sharedir + '/icons/hicolor'
bindir := rootdir + prefix + '/bin'

build: _extract_vendor
    #!/usr/bin/env bash
    cargo build {{cargo_args}}

# Compiles with debug profile
build-debug *args:
    cargo build {{args}}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

_install_icons name:
    find {{name}}/'data'/'icons' -type f -exec echo {} \; | rev | cut -d'/' -f-3 | rev | xargs -d '\n' -I {} install -Dm0644 {{name}}/'data'/'icons'/{} {{iconsdir}}/{}

_install_desktop path:
    install -Dm0644 {{path}} {{sharedir}}/applications/{{file_name(path)}}

_install_bin name:
    install -Dm0755 target/{{target}}/{{name}} {{bindir}}/{{name}}

_install id name: (_install_icons name) (_install_desktop name + '/data/' + id + '.desktop') (_install_bin name)

_install_app_list: (_install 'com.system76.CosmicAppList' 'cosmic-app-list')
_install_audio: (_install 'com.system76.CosmicAppletAudio' 'cosmic-applet-audio')
_install_battery: (_install 'com.system76.CosmicAppletBattery' 'cosmic-applet-battery')
_install_bluetooth: (_install 'com.system76.CosmicAppletBluetooth' 'cosmic-applet-bluetooth')
_install_graphics: (_install 'com.system76.CosmicAppletGraphics' 'cosmic-applet-graphics')
_install_network: (_install 'com.system76.CosmicAppletNetwork' 'cosmic-applet-network')
_install_notifications: (_install 'com.system76.CosmicAppletNotifications' 'cosmic-applet-notifications')
_install_power: (_install 'com.system76.CosmicAppletPower' 'cosmic-applet-power')
_install_workspace: (_install 'com.system76.CosmicAppletWorkspaces' 'cosmic-applet-workspaces')
_install_time: (_install 'com.system76.CosmicAppletTime' 'cosmic-applet-time')
_install_tiling: (_install 'com.system76.CosmicAppletTiling' 'cosmic-applet-tiling')
_install_status_area: (_install 'com.system76.CosmicAppletStatusArea' 'cosmic-applet-status-area')

# TODO: Turn this into one configurable applet?
_install_panel_button: (_install_bin 'cosmic-panel-button')
_install_button id name: (_install_icons name) (_install_desktop name + '/data/' + id + '.desktop')
_install_app_button: (_install_button 'com.system76.CosmicPanelAppButton' 'cosmic-panel-app-button')
_install_workspaces_button: (_install_button 'com.system76.CosmicPanelWorkspacesButton' 'cosmic-panel-workspaces-button')

# Installs files into the system
install: _install_app_list _install_audio _install_battery _install_bluetooth _install_graphics _install_network _install_notifications _install_power _install_workspace _install_time _install_tiling _install_panel_button _install_app_button _install_workspaces_button _install_status_area

# Extracts vendored dependencies if vendor=1
_extract_vendor:
    #!/usr/bin/env sh
    if test {{vendor}} = 1; then
        rm -rf vendor; tar pxf vendor.tar
    fi
