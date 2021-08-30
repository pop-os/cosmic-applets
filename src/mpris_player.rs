use cascade::cascade;
use gtk4::{
    gdk, gdk_pixbuf, gio,
    glib::{self, clone},
    pango,
    prelude::*,
    subclass::prelude::*,
};
use std::cell::RefCell;

use crate::deref_cell::DerefCell;

#[derive(Default)]
pub struct MprisPlayerInner {
    box_: DerefCell<gtk4::Box>,
    backward_button: DerefCell<gtk4::Button>,
    play_pause_button: DerefCell<gtk4::Button>,
    forward_button: DerefCell<gtk4::Button>,
    player: DerefCell<Player>,
    picture: DerefCell<gtk4::Picture>,
    picture_uri: RefCell<Option<String>>,
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
        let picture = cascade! {
            gtk4::Picture::new();
            ..set_halign(gtk4::Align::Center);
            ..set_valign(gtk4::Align::Center);
            ..set_can_shrink(true);
            ..set_size_request(32, 32);
        };

        let title_label = cascade! {
            gtk4::Label::new(None);
            ..set_ellipsize(pango::EllipsizeMode::End);
            ..set_max_width_chars(20);
            ..set_attributes(Some(&cascade! {
                pango::AttrList::new();
                ..insert(pango::Attribute::new_weight(pango::Weight::Bold));
            }));
        };

        let artist_label = cascade! {
            gtk4::Label::new(None);
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
            gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            ..set_parent(obj);
            ..append(&picture);
            ..append(&title_label);
            ..append(&artist_label);
            ..append(&cascade! {
                gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                ..set_valign(gtk4::Align::Start);
                ..append(&backward_button);
                ..append(&play_pause_button);
                ..append(&forward_button);
            });
        };

        self.box_.set(box_);
        self.backward_button.set(backward_button);
        self.play_pause_button.set(play_pause_button);
        self.forward_button.set(forward_button);
        self.picture.set(picture);
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
    pub async fn new(name: &str) -> Result<Self, glib::Error> {
        let obj = glib::Object::new::<Self>(&[]).unwrap();

        let player = Player::new(name).await?;
        player.connect_properties_changed(clone!(@weak obj => move |_player| {
            obj.update();
        }));
        obj.inner().player.set(player);
        obj.update();

        Ok(obj)
    }

    fn inner(&self) -> &MprisPlayerInner {
        MprisPlayerInner::from_instance(self)
    }

    fn call(&self, method: &'static str) {
        glib::MainContext::default().spawn_local(clone!(@strong self as self_ => async move {
            if let Err(err) = self_.inner().player.call(method).await {
                eprintln!("Failed to call '{}': {}", method, err);
            }
        }));
    }

    async fn update_arturl(&self, arturl: Option<&str>) {
        let mut picture_uri = self.inner().picture_uri.borrow_mut();
        if picture_uri.as_deref() == arturl {
            return;
        }
        *picture_uri = arturl.map(String::from);
        drop(picture_uri);

        let pixbuf = async {
            let file = gio::File::for_uri(&arturl?);
            let stream = file.read_async_future(glib::PRIORITY_DEFAULT).await.ok()?;
            gdk_pixbuf::Pixbuf::from_stream_at_scale_async_future(&stream, 256, 256, false)
                .await
                .ok()
        }
        .await;
        if let Some(pixbuf) = pixbuf {
            let texture = gdk::Texture::for_pixbuf(&pixbuf);
            self.inner().picture.set_paintable(Some(&texture));
        }
    }

    fn update(&self) {
        let player = &self.inner().player;

        // XXX status
        let (status, metadata) = match (player.playback_status(), player.metadata()) {
            (Some(status), Some(metadata)) => (status, metadata),
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

    fn connect_properties_changed<F: Fn(Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let proxy = &self.0;
        self.0
            .connect_local(
                "g-properties-changed",
                false,
                clone!(@weak proxy => @default-panic, move |_| {
                    f(Self(proxy));
                    None
                }),
            )
            .unwrap()
    }

    fn playback_status(&self) -> Option<String> {
        self.property("PlaybackStatus")
    }

    fn metadata(&self) -> Option<Metadata> {
        Some(Metadata(self.property("Metadata")?))
    }
}
