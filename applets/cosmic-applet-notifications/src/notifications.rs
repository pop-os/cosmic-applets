#![allow(non_snake_case)]

use futures::channel::mpsc;
use futures::stream::StreamExt;
use gtk4::{
    glib::{self, clone, subclass::Signal, SignalHandlerId},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;
use once_cell::unsync::OnceCell;
use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};
use zbus::{dbus_interface, Result, SignalContext};
use zvariant::OwnedValue;

use crate::dbus_service;
use crate::deref_cell::DerefCell;

static PATH: &str = "/org/freedesktop/Notifications";
static INTERFACE: &str = "org.freedesktop.Notifications";

enum Event {
    NotificationReceived(NotificationId),
    CloseNotification(NotificationId),
}

pub struct NotificationsInterfaceInner {
    next_id: Mutex<NotificationId>,
    notifications: Mutex<HashMap<NotificationId, Arc<Notification>>>,
    sender: mpsc::UnboundedSender<Event>,
}

#[derive(Clone)]
pub struct NotificationsInterface(Arc<NotificationsInterfaceInner>);

impl NotificationsInterface {
    fn new() -> (Self, mpsc::UnboundedReceiver<Event>) {
        let (sender, receiver) = mpsc::unbounded();
        (
            Self(Arc::new(NotificationsInterfaceInner {
                next_id: Default::default(),
                notifications: Default::default(),
                sender,
            })),
            receiver,
        )
    }

    fn next_id(&self) -> NotificationId {
        let mut next_id = self.0.next_id.lock().unwrap();
        let id = *next_id;
        *next_id = NotificationId::new(u32::from(id).wrapping_add(1)).unwrap_or_default();
        id
    }

    fn handle_notify(
        &self,
        app_name: String,
        replaces_id: Option<NotificationId>,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: Hints,
        _expire_timeout: i32,
    ) -> NotificationId {
        // Ignores `expire-timeout`, like Gnome Shell

        let id = replaces_id.unwrap_or_else(|| self.next_id());

        let notification = Arc::new(Notification {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions,
            hints,
        });

        self.0
            .notifications
            .lock()
            .unwrap()
            .insert(id, notification);

        self.0
            .sender
            .unbounded_send(Event::NotificationReceived(id))
            .unwrap();

        id
    }
}

// TODO: return value variable names in introspection data?

#[dbus_interface(name = "org.freedesktop.Notifications")]
impl NotificationsInterface {
    fn Notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: Hints,
        expire_timeout: i32,
    ) -> u32 {
        u32::from(self.handle_notify(
            app_name,
            NotificationId::new(replaces_id),
            app_icon,
            summary,
            body,
            actions,
            hints,
            expire_timeout,
        ))
    }

    async fn CloseNotification(&self, id: u32) {
        if let Some(id) = NotificationId::new(id) {
            self.0
                .sender
                .unbounded_send(Event::CloseNotification(id))
                .unwrap();
        }
        // TODO error?
    }

    fn GetCapabilities(&self) -> Vec<&'static str> {
        // TODO: body-markup, sound
        vec!["actions", "body", "icon-static", "persistence"]
    }

    fn GetServerInformation(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("cosmic-panel", "system76", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[dbus_interface(signal)]
    async fn NotificationClosed(ctxt: &SignalContext<'_>, id: u32, reason: u32) -> Result<()>;

    #[dbus_interface(signal)]
    async fn ActionInvoked(ctxt: &SignalContext<'_>, id: u32, action_key: &str) -> Result<()>;
}

#[derive(Default)]
pub struct NotificationsInner {
    interface: DerefCell<NotificationsInterface>,
    connection: OnceCell<zbus::Connection>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationsInner {
    const NAME: &'static str = "S76Notifications";
    type ParentType = glib::Object;
    type Type = Notifications;
}

impl ObjectImpl for NotificationsInner {
    fn signals() -> &'static [Signal] {
        static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
            vec![
                Signal::builder("notification-received")
                    .param_types(&[NotificationId::static_type().into()])
                    .build(),
                Signal::builder("notification-closed")
                    .param_types(&[NotificationId::static_type().into()])
                    .build(),
            ]
        });
        SIGNALS.as_ref()
    }
}

glib::wrapper! {
    pub struct Notifications(ObjectSubclass<NotificationsInner>);
}

#[derive(zvariant::Type, serde::Deserialize)]
struct Hints(HashMap<String, OwnedValue>);

#[allow(dead_code)]
impl Hints {
    fn prop<T: TryFrom<OwnedValue>>(&self, name: &str) -> Option<T> {
        T::try_from(self.0.get(name)?.clone()).ok()
    }

    fn actions_icon(&self) -> bool {
        self.prop("actions-icon").unwrap_or(false)
    }

    fn category(&self) -> Option<String> {
        self.prop("category")
    }

    fn desktop_entry(&self) -> Option<String> {
        self.prop("desktop-entry")
    }

    fn image_data(&self) -> Option<(i32, i32, i32, bool, i32, i32, Vec<u8>)> {
        self.prop("image-data")
            .or_else(|| self.prop("image_data"))
            .or_else(|| self.prop("icon_data"))
    }

    fn image_path(&self) -> Option<String> {
        self.prop("image-path").or_else(|| self.prop("image_path"))
    }

    fn resident(&self) -> bool {
        self.prop("resident").unwrap_or(false)
    }

    fn sound_file(&self) -> Option<String> {
        self.prop("sound-file")
    }

    fn sound_name(&self) -> Option<String> {
        self.prop("sound-name")
    }

    fn transient(&self) -> bool {
        self.prop("transient").unwrap_or(false)
    }

    fn xy(&self) -> Option<(u8, u8)> {
        Some((self.prop("x")?, self.prop("y")?))
    }

    fn urgency(&self) -> Option<u8> {
        self.prop("urgency")
    }
}

impl fmt::Debug for Hints {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = f.debug_struct("Hints");
        for (k, v) in &self.0 {
            if let Ok(v) = <&str>::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = i32::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = bool::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = u8::try_from(v) {
                s.field(k, &v);
            } else {
                s.field(k, v);
            };
        }
        s.finish()
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, glib::Boxed, PartialEq, Eq)]
#[boxed_type(name = "S76NotificationId")]
pub struct NotificationId(NonZeroU32);

impl Default for NotificationId {
    fn default() -> Self {
        Self(NonZeroU32::new(1).unwrap())
    }
}

impl From<NotificationId> for u32 {
    fn from(id: NotificationId) -> u32 {
        id.0.into()
    }
}

impl NotificationId {
    fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    pub app_icon: String, // decode?
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>, // enum?
    hints: Hints,
}

#[repr(u32)]
#[allow(dead_code)]
enum CloseReason {
    Expire = 1,
    Dismiss,
    Call,
    Undefined,
}

impl Notifications {
    pub fn new() -> Self {
        let notifications = glib::Object::new::<Self>(&[]).unwrap();

        let (interface, mut receiver) = NotificationsInterface::new();
        notifications.inner().interface.set(interface);

        glib::MainContext::default().spawn_local(clone!(@strong notifications => async move {
            let connection = match dbus_service::create(INTERFACE, |builder| builder.serve_at(PATH, notifications.inner().interface.clone())).await {
                Ok(connection) => connection,
                Err(err) => {
                    eprintln!("Failed to start `Notifications` service: {}", err);
                    return;
                }
            };
            let _ = notifications.inner().connection.set(connection.clone());

            while let Some(event) = receiver.next().await {
                match event {
                    Event::NotificationReceived(id) => {
                        notifications.emit_by_name::<()>("notification-received", &[&id]);
                    }
                    Event::CloseNotification(id) =>  {
                        notifications.close_notification(id, CloseReason::Call).await
                    }
                }
            }
        }));

        notifications
    }

    fn inner(&self) -> &NotificationsInner {
        NotificationsInner::from_instance(self)
    }

    async fn close_notification(&self, id: NotificationId, reason: CloseReason) {
        self.inner()
            .interface
            .0
            .notifications
            .lock()
            .unwrap()
            .remove(&id);

        self.emit_by_name::<()>("notification-closed", &[&id]);

        if let Some(connection) = self.inner().connection.get() {
            let ctxt = SignalContext::new(connection, PATH).unwrap(); // XXX unwrap?
            let _ =
                NotificationsInterface::NotificationClosed(&ctxt, id.into(), reason as u32).await;
        }
    }

    pub fn dismiss(&self, id: NotificationId) {
        glib::MainContext::default().spawn_local(clone!(@strong self as self_ => async move {
            self_.close_notification(id, CloseReason::Dismiss).await
        }));
    }

    pub async fn invoke_action(&self, id: NotificationId, action_key: &str) {
        if let Some(connection) = self.inner().connection.get() {
            let ctxt = SignalContext::new(connection, PATH).unwrap(); // XXX unwrap?
            let _ = NotificationsInterface::ActionInvoked(&ctxt, id.into(), action_key).await;
        }
    }

    pub fn get(&self, id: NotificationId) -> Option<Arc<Notification>> {
        self.inner()
            .interface
            .0
            .notifications
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
    }

    pub fn connect_notification_received<F: Fn(Arc<Notification>) + 'static>(
        &self,
        cb: F,
    ) -> SignalHandlerId {
        self.connect_local("notification-received", false, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let id = values[1].get().unwrap();
            if let Some(notification) = obj.get(id) {
                cb(notification);
            }
            None
        })
    }

    pub fn connect_notification_closed<F: Fn(NotificationId) + 'static>(
        &self,
        cb: F,
    ) -> SignalHandlerId {
        self.connect_local("notification-closed", false, move |values| {
            let id = values[1].get().unwrap();
            cb(id);
            None
        })
    }
}
