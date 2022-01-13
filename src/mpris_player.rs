use cascade::cascade;
use futures::StreamExt;
use gtk4::{
    gdk_pixbuf, gio,
    glib::{self, clone},
    pango,
    prelude::*,
    subclass::prelude::*,
};
use std::{cell::RefCell, collections::HashMap};
use zbus::dbus_proxy;
use zvariant::OwnedValue;

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct MprisPlayerInner {
    box_: DerefCell<gtk4::Box>,
    backward_button: DerefCell<gtk4::Button>,
    play_pause_button: DerefCell<gtk4::Button>,
    forward_button: DerefCell<gtk4::Button>,
    player: DerefCell<PlayerProxy<'static>>,
    image: DerefCell<gtk4::Image>,
    image_uri: RefCell<Option<String>>,
    title_label: DerefCell<gtk4::Label>,
    artist_label: DerefCell<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for MprisPlayerInner {
    const NAME: &'static str = "S76MprisPlayer";
    type ParentType = gtk4::Widget;
    type Type = MprisPlayer;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
    }
}

impl ObjectImpl for MprisPlayerInner {
    fn constructed(&self, obj: &MprisPlayer) {
        let image = cascade! {
            gtk4::Image::new();
            ..set_pixel_size(64);
        };

        let title_label = cascade! {
            gtk4::Label::new(None);
            ..set_halign(gtk4::Align::Start);
            ..set_ellipsize(pango::EllipsizeMode::End);
            ..set_max_width_chars(20);
            ..set_attributes(Some(&cascade! {
                pango::AttrList::new();
                ..insert(pango::Attribute::new_weight(pango::Weight::Bold));
            }));
        };

        let artist_label = cascade! {
            gtk4::Label::new(None);
            ..set_halign(gtk4::Align::Start);
            ..set_ellipsize(pango::EllipsizeMode::End);
            ..set_max_width_chars(20);
        };

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
            gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            ..set_parent(obj);
            ..append(&image);
            ..append(&cascade! {
                gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                ..append(&title_label);
                ..append(&artist_label);
                ..append(&cascade! {
                    gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                    ..set_valign(gtk4::Align::Start);
                    ..append(&backward_button);
                    ..append(&play_pause_button);
                    ..append(&forward_button);
                });
            });
        };

        self.box_.set(box_);
        self.backward_button.set(backward_button);
        self.play_pause_button.set(play_pause_button);
        self.forward_button.set(forward_button);
        self.image.set(image);
        self.title_label.set(title_label);
        self.artist_label.set(artist_label);
    }

    fn dispose(&self, _obj: &MprisPlayer) {
        self.box_.unparent();
    }
}

impl WidgetImpl for MprisPlayerInner {}

glib::wrapper! {
    pub struct MprisPlayer(ObjectSubclass<MprisPlayerInner>)
        @extends gtk4::Widget;
}

impl MprisPlayer {
    pub async fn new(name: &str) -> zbus::Result<Self> {
        let obj = glib::Object::new::<Self>(&[]).unwrap();

        let connection = zbus::Connection::session().await?;
        let player = PlayerProxy::builder(&connection)
            .destination(name.to_string())?
            .build()
            .await?;

        let metadata_stream = player.receive_metadata_changed().await;
        let playback_status_stream = player.receive_playback_status_changed().await;
        let mut stream = futures::stream_select!(
            metadata_stream.map(|_| ()),
            playback_status_stream.map(|_| ())
        );

        obj.inner().player.set(player);

        glib::MainContext::default().spawn_local(clone!(@strong obj => async move {
            while stream.next().await.is_some() {
                obj.update();
            }
        }));
        obj.update();

        Ok(obj)
    }

    fn inner(&self) -> &MprisPlayerInner {
        MprisPlayerInner::from_instance(self)
    }

    fn call(&self, method: &'static str) {
        glib::MainContext::default().spawn_local(clone!(@strong self as self_ => async move {
            if let Err(err) = self_.inner().player.call::<_, _, ()>(method, &()).await {
                eprintln!("Failed to call '{}': {}", method, err);
            }
        }));
    }

    async fn update_arturl(&self, arturl: Option<&str>) {
        let mut image_uri = self.inner().image_uri.borrow_mut();
        if image_uri.as_deref() == arturl {
            return;
        }
        *image_uri = arturl.map(String::from);
        drop(image_uri);

        let pixbuf = async {
            // TODO: Security?
            let file = gio::File::for_uri(&arturl?);
            let stream = file.read_async_future(glib::PRIORITY_DEFAULT).await.ok()?;
            gdk_pixbuf::Pixbuf::from_stream_async_future(&stream)
                .await
                .ok()
        }
        .await;
        if let Some(pixbuf) = pixbuf {
            self.inner().image.set_from_pixbuf(Some(&pixbuf));
        }
    }

    fn update(&self) {
        let player = &self.inner().player;
        let (status, metadata) = match (player.cached_playback_status(), player.cached_metadata()) {
            (Ok(Some(status)), Ok(Some(metadata))) => (status, metadata),
            _ => return,
        };

        let play_pause_icon = if status == "Playing" {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };

        let title = metadata.title().unwrap_or_else(|| String::new());
        // XXX correct way to handle multiple?
        let artist = metadata
            .artist()
            .and_then(|x| x.get(0).cloned())
            .unwrap_or_default();

        let _album = metadata.album(); // TODO

        let arturl = metadata.arturl();
        glib::MainContext::default().spawn_local(clone!(@strong self as self_ => async move {
            self_.update_arturl(arturl.as_deref()).await;
        }));

        self.inner()
            .play_pause_button
            .set_icon_name(play_pause_icon);
        self.inner().title_label.set_label(&title);
        self.inner().artist_label.set_label(&artist);
    }
}

pub struct Metadata(HashMap<String, OwnedValue>);

impl TryFrom<OwnedValue> for Metadata {
    type Error = zbus::Error;

    fn try_from(value: OwnedValue) -> zbus::Result<Self> {
        Ok(Self(value.try_into()?))
    }
}

impl Metadata {
    fn lookup<'a, T: TryFrom<OwnedValue>>(&self, key: &str) -> Option<T> {
        T::try_from(self.0.get(key)?.clone()).ok()
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

#[dbus_proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    #[dbus_proxy(property)]
    fn metadata(&self) -> zbus::Result<Metadata>;

    #[dbus_proxy(property)]
    fn playback_status(&self) -> zbus::Result<String>;
}
