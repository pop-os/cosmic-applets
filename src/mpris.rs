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
use crate::mpris_player::MprisPlayer;

#[derive(Default)]
pub struct MprisControlsInner {
    box_: DerefCell<gtk4::Box>,
    dbus: OnceCell<DBus>,
    players: RefCell<HashMap<String, MprisPlayer>>,
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
        let box_ = cascade! {
            gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            ..set_parent(obj);
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

    async fn player_added(&self, name: &str) {
        let player = match MprisPlayer::new(&name).await {
            Ok(player) => player,
            Err(err) => {
                eprintln!("Failed to connect to '{}': {}", name, err);
                return;
            }
        };

        self.inner().box_.append(&player); // XXX

        self.inner()
            .players
            .borrow_mut()
            .insert(name.to_owned(), player.clone());
    }

    fn player_removed(&self, name: &str) {
        self.inner().players.borrow_mut().remove(name);
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

    fn connect_name_owner_changed<F: Fn(String, String, String) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.0
            .connect_local("g-signal", false, move |args| {
                if &args[2].get::<String>().unwrap() == "NameOwnerChanged" {
                    let (name, old, new) = args[3].get::<glib::Variant>().unwrap().get().unwrap();
                    f(name, old, new);
                }
                None
            })
            .unwrap()
    }
}
