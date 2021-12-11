use cascade::cascade;
use futures::stream::StreamExt;
use gtk4::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::unsync::OnceCell;
use std::{cell::RefCell, collections::HashMap};
use zbus::fdo::DBusProxy;

use crate::deref_cell::DerefCell;
use crate::mpris_player::MprisPlayer;

#[derive(Default)]
pub struct MprisControlsInner {
    listbox: DerefCell<gtk4::ListBox>,
    dbus: OnceCell<DBusProxy<'static>>,
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
        let listbox = cascade! {
            gtk4::ListBox::new();
            ..set_parent(obj);
        };

        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            let (dbus, mut name_owner_changed_stream) = match async {
                let connection = zbus::Connection::session().await?;
                let dbus = DBusProxy::new(&connection).await?;
                let stream = dbus.receive_name_owner_changed().await?;
                Ok::<_, zbus::Error>((dbus, stream))
            }.await {
                Ok(value) => value,
                Err(err) => {
                    eprintln!("Failed to connect to 'org.freedesktop.DBus': {}", err);
                    return;
                }
            };

            glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
                while let Some(evt) = name_owner_changed_stream.next().await {
                    let args = match evt.args() {
                        Ok(args) => args,
                        Err(_) => { continue; },
                    };
                    if args.name.starts_with("org.mpris.MediaPlayer2.") {
                        if !args.old_owner.is_none() {
                            obj.player_removed(&args.name);
                        }
                        if !args.new_owner.is_none() {
                            obj.player_added(&args.name).await;
                        }
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

        self.listbox.set(listbox);
    }

    fn dispose(&self, _obj: &MprisControls) {
        self.listbox.unparent();
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

        let row = cascade! {
            gtk4::ListBoxRow::new();
            ..set_selectable(false);
            ..set_child(Some(&player));
        };
        self.inner().listbox.append(&row);

        self.inner()
            .players
            .borrow_mut()
            .insert(name.to_owned(), player.clone());
    }

    fn player_removed(&self, name: &str) {
        self.inner().players.borrow_mut().remove(name);
    }
}
