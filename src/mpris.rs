use cascade::cascade;
use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::unsync::OnceCell;
use std::{cell::RefCell, collections::HashMap};

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct MprisControlsInner {
    box_: DerefCell<gtk4::Box>,
    backward_button: DerefCell<gtk4::Button>,
    play_pause_button: DerefCell<gtk4::Button>,
    forward_button: DerefCell<gtk4::Button>,
    dbus: OnceCell<DBus>,
    players: RefCell<HashMap<String, Player>>,
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
            ..connect_clicked(clone!(@strong obj => move |_| obj.call("Previous")));
        };

        let play_pause_button = cascade! {
            gtk4::Button::from_icon_name(Some("media-playback-start-symbolic"));
            ..connect_clicked(clone!(@strong obj => move |_| obj.call("PlayPause")));
        };

        let forward_button = cascade! {
            gtk4::Button::from_icon_name(Some("media-skip-forward-symbolic"));
            ..connect_clicked(clone!(@strong obj => move |_| obj.call("Next")));
        };

        let box_ = cascade! {
            gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            ..set_parent(obj);
            ..append(&backward_button);
            ..append(&play_pause_button);
            ..append(&forward_button);
        };

        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            let dbus = match DBus::new().await {
                Ok(dbus) => dbus,
                Err(err) => {
                    eprintln!("Failed to connect to 'org.freedesktop.DBus': {}", err);
                    return;
                }
            };

            dbus.connect_name_owner_changed(clone!(@strong obj => move |name, old, new| {
                if name.starts_with("org.mpris.MediaPlayer2.") {
                    if !old.is_empty() {
                        obj.player_removed(&name);
                    }
                    if !new.is_empty() {
                        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                        obj.player_added(&name).await;
                        }));
                    }
                }
            }));

            match dbus.list_names().await {
                Ok(names) => for name in names {
                    if name.starts_with("org.mpris.MediaPlayer2.") {
                        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                        obj.player_added(&name).await;
                        }));
                    }
                }
                Err(err) => eprintln!("Failed to call 'ListNames: {}'", err)
            }

            let _ = obj.inner().dbus.set(dbus);
        }));

        self.box_.set(box_);
        self.backward_button.set(backward_button);
        self.play_pause_button.set(play_pause_button);
        self.forward_button.set(forward_button);
    }

    fn dispose(&self, _obj: &MprisControls) {
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

    fn inner(&self) -> &MprisControlsInner {
        MprisControlsInner::from_instance(self)
    }

    fn player(&self) -> Option<Player> {
        // XXX correctly choose which player to show
        self.inner().players.borrow().values().next().cloned()
    }

    async fn player_added(&self, name: &str) {
        let player = match Player::new(&name).await {
            Ok(player) => player,
            Err(err) => {
                eprintln!("Failed to connect to '{}': {}", name, err);
                return;
            }
        };

        let status = player.playback_status();
        let metadata = player.metadata().unwrap(); // XXX unwrap
        let title = metadata.title();
        let album = metadata.album();
        let artist = metadata.artist();
        let arturl = metadata.arturl();
        println!("{:?}", (status, title, album, artist, arturl));

        self.inner()
            .players
            .borrow_mut()
            .insert(name.to_owned(), player);
    }

    fn player_removed(&self, name: &str) {
        self.inner().players.borrow_mut().remove(name);
    }

    fn call(&self, method: &'static str) {
        glib::MainContext::default().spawn_local(clone!(@strong self as self_ => async move {
            if let Some(player) = self_.player() {
                if let Err(err) = player.call(method).await {
                    eprintln!("Failed to call '{}': {}", method, err);
                }
            }
        }));
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

#[derive(Clone)]
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

    async fn call(&self, method: &str) -> Result<(), glib::Error> {
        self.0
            .call_future(method, None, gio::DBusCallFlags::NONE, 1000)
            .await?;
        Ok(())
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
