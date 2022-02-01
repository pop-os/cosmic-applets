use gtk4::{
    gdk::Display, gio::ApplicationFlags, prelude::*, CssProvider, StyleContext,
    STYLE_PROVIDER_PRIORITY_APPLICATION,
};

fn main() {
    let application = gtk4::Application::new(
        Some("com.system76.cosmic.applets.network"),
        ApplicationFlags::default(),
    );
    application.connect_activate(build_ui);
    application.run();
}

fn build_ui(application: &gtk4::Application) {
    let window = gtk4::ApplicationWindow::builder()
        .application(application)
        .title("COSMIC Network Applet")
        .default_width(400)
        .default_height(600)
        .build();
}
