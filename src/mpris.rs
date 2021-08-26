use cascade::cascade;
use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use std::cell::RefCell;

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct MprisControlsInner {
    box_: DerefCell<gtk4::Box>,
    backward_button: DerefCell<gtk4::Button>,
    play_pause_button: DerefCell<gtk4::Button>,
    forward_button: DerefCell<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for MprisControlsInner {
    const NAME: &'static str = "S76MprisControls";
    type ParentType = gtk4::Widget;
    type Type = MprisControls;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for MprisControlsInner {
    fn constructed(&self, obj: &MprisControls) {
        let backward_button = cascade! {
            gtk4::Button::from_icon_name(Some("media-skip-backward-symbolic"));
            ..connect_clicked(|_| {});
        };

        let play_pause_button = cascade! {
            gtk4::Button::from_icon_name(Some("media-playback-start-symbolic"));
            ..connect_clicked(|_| {});
        };

        let forward_button = cascade! {
            gtk4::Button::from_icon_name(Some("media-skip-forward-symbolic"));
            ..connect_clicked(|_| {});
        };

        let box_ = cascade! {
            gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            ..set_parent(obj);
            ..append(&backward_button);
            ..append(&play_pause_button);
            ..append(&forward_button);
        };

        /*
        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            // XXX unwrap
            let dbus = DBus::new().await.unwrap();
            dbus.connect_name_owner_changed(|a, b, c| {
                println!("{:?}", (a, b, c));
            });
            for name in dbus.list_names().await.unwrap() {
                if !name.starts_with("org.mpris.MediaPlayer2.") {
                    continue;
                }
                let player = Player::new(&name).await.unwrap();

                println!("{}", name);
                let status = player.playback_status();
                let metadata = player.metadata().unwrap();
                let title = metadata.title();
                let album = metadata.album();
                let artist = metadata.artist();
                let art = metadata.arturl();
                println!("{:?}", (title, art));
            }
            std::mem::forget(dbus);
        }));
        */

        self.box_.set(box_);
        self.backward_button.set(backward_button);
        self.play_pause_button.set(play_pause_button);
        self.forward_button.set(forward_button);
    }

    fn dispose(&self, obj: &MprisControls) {
        self.box_.unparent();
    }
}

impl WidgetImpl for MprisControlsInner {}

glib::wrapper! {
    pub struct MprisControls(ObjectSubclass<MprisControlsInner>)
        @extends gtk4::Widget;
}

impl MprisControls {
    pub fn new() -> Self {
        glib::Object::new(&[]).unwrap()
    }
}

struct Metadata(glib::VariantDict);

impl Metadata {
    fn lookup<T: glib::FromVariant>(&self, key: &str) -> Option<T> {
        self.0.lookup_value(key, None)?.get()
    }

    fn title(&self) -> Option<String> {
        self.lookup("xesam:title")
    }

    fn album(&self) -> Option<String> {
        self.lookup("xesam:album")
    }

    fn artist(&self) -> Option<Vec<String>> {
        self.lookup("xesam:artist")
    }

    fn arturl(&self) -> Option<String> {
        self.lookup("mpris:artUrl")
    }
}

struct Player(gio::DBusProxy);

impl Player {
    async fn new(name: &str) -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            name,
            "/org/mpris/MediaPlayer2",
            "org.mpris.MediaPlayer2.Player",
        )
        .await?;
        Ok(Self(proxy))
    }

    fn property<T: glib::FromVariant>(&self, prop: &str) -> Option<T> {
        self.0.cached_property(prop)?.get()
    }

    fn connect_properties_changed<F: Fn() + 'static>(&self, f: F) {
        self.0
            .connect_local("g-properties-changed", false, move |_| {
                f();
                None
            })
            .unwrap();
    }

    fn playback_status(&self) -> Option<String> {
        self.property("PlayBackStatus")
    }

    fn metadata(&self) -> Option<Metadata> {
        Some(Metadata(self.property("Metadata")?))
    }
}

struct DBus(gio::DBusProxy);

impl DBus {
    async fn new() -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        Ok(Self(proxy))
    }

    async fn list_names(&self) -> Result<impl Iterator<Item = String>, glib::Error> {
        Ok(self
            .0
            .call_future("ListNames", None, gio::DBusCallFlags::NONE, 1000)
            .await?
            .child_value(0)
            .iter()
            .filter_map(|x| x.get::<String>()))
    }

    fn connect_name_owner_changed<F: Fn(String, String, String) + 'static>(&self, f: F) {
        self.0
            .connect_local("g-signal", false, move |args| {
                if &args[2].get::<String>().unwrap() == "NameOwnerChanged" {
                    let (name, old, new) = args[3].get::<glib::Variant>().unwrap().get().unwrap();
                    f(name, old, new);
                }
                None
            })
            .unwrap();
    }
}
