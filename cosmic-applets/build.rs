use std::fs;
use xdgen::{App, Context, FluentString};

fn main() {
    let ctx = Context::new("../i18n/", "desktop_entries").unwrap();

    [
        (
            "com.system76.CosmicAppList",
            "cosmic-app-list",
            "cosmic-app-list-comment",
            "cosmic-app-list-keywords",
        ),
        (
            "com.system76.CosmicAppletA11y",
            "cosmic-applet-a11y",
            "cosmic-applet-a11y-comment",
            "cosmic-applet-a11y-keywords",
        ),
        (
            "com.system76.CosmicAppletAudio",
            "cosmic-applet-audio",
            "cosmic-applet-audio-comment",
            "cosmic-applet-audio-keywords",
        ),
        (
            "com.system76.CosmicAppletBattery",
            "cosmic-applet-battery",
            "cosmic-applet-battery-comment",
            "cosmic-applet-battery-keywords",
        ),
        (
            "com.system76.CosmicAppletBluetooth",
            "cosmic-applet-bluetooth",
            "cosmic-applet-bluetooth-comment",
            "cosmic-applet-bluetooth-keywords",
        ),
        (
            "com.system76.CosmicAppletInputSources",
            "cosmic-applet-input-sources",
            "cosmic-applet-input-sources-comment",
            "cosmic-applet-input-sources-keywords",
        ),
        (
            "com.system76.CosmicAppletMinimize",
            "cosmic-applet-minimize",
            "cosmic-applet-minimize-comment",
            "cosmic-applet-minimize-keywords",
        ),
        (
            "com.system76.CosmicAppletNetwork",
            "cosmic-applet-network",
            "cosmic-applet-network-comment",
            "cosmic-applet-network-keywords",
        ),
        (
            "com.system76.CosmicAppletNotifications",
            "cosmic-applet-notifications",
            "cosmic-applet-notifications-comment",
            "cosmic-applet-notifications-keywords",
        ),
        (
            "com.system76.CosmicAppletPower",
            "cosmic-applet-power",
            "cosmic-applet-power-comment",
            "cosmic-applet-power-keywords",
        ),
        (
            "com.system76.CosmicAppletStatusArea",
            "cosmic-applet-status-area",
            "cosmic-applet-status-area-comment",
            "cosmic-applet-status-area-keywords",
        ),
        (
            "com.system76.CosmicAppletTiling",
            "cosmic-applet-tiling",
            "cosmic-applet-tiling-comment",
            "cosmic-applet-tiling-keywords",
        ),
        (
            "com.system76.CosmicAppletTime",
            "cosmic-applet-time",
            "cosmic-applet-time-comment",
            "cosmic-applet-time-keywords",
        ),
        (
            "com.system76.CosmicAppletWorkspaces",
            "cosmic-applet-workspaces",
            "cosmic-applet-workspaces-comment",
            "cosmic-applet-workspaces-keywords",
        ),
        (
            "com.system76.CosmicPanelAppButton",
            "cosmic-panel-app-button",
            "cosmic-panel-app-button-comment",
            "cosmic-panel-app-button-keywords",
        ),
        (
            "com.system76.CosmicPanelLauncherButton",
            "cosmic-panel-launcher-button",
            "cosmic-panel-launcher-button-comment",
            "cosmic-panel-launcher-button-keywords",
        ),
        (
            "com.system76.CosmicPanelWorkspacesButton",
            "cosmic-panel-workspaces-button",
            "cosmic-panel-workspaces-button-comment",
            "cosmic-panel-workspaces-button-keywords",
        ),
    ]
    .into_iter()
    .map(|(id, name, comment, keywords)| {
        let template_path = ["../", name, "/data/", id, ".desktop"].concat();

        let app = App::new(FluentString(name))
            .comment(FluentString(comment))
            .keywords(FluentString(keywords));

        (id, app.expand_desktop(&template_path, &ctx).unwrap())
    })
    .for_each(|(id, contents)| {
        let parent = "../target/xdgen/";
        fs::create_dir_all(parent).unwrap();
        fs::write([parent, id, ".desktop"].concat().as_str(), contents).unwrap();
    });
}
