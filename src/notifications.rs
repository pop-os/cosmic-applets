use gtk4::{
    gio,
    glib::{self, clone, subclass::Signal, SignalHandlerId},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt,
    num::NonZeroU32,
    rc::Rc,
    time::Duration,
};

static NOTIFICATIONS_XML: &str = "
<node name='/org/freedesktop/Notifications'>
  <interface name='org.freedesktop.Notifications'>
    <method name='Notify'>
      <arg type='s' name='app_name' direction='in'/>
      <arg type='u' name='replaces_id' direction='in'/>
      <arg type='s' name='app_icon' direction='in'/>
      <arg type='s' name='summary' direction='in'/>
      <arg type='s' name='body' direction='in'/>
      <arg type='as' name='actions' direction='in'/>
      <arg type='a{sv}' name='hints' direction='in'/>
      <arg type='i' name='expire_timeout' direction='in'/>
      <arg type='u' name='id' direction='out'/>
    </method>

    <method name='CloseNotification'>
      <arg type='u' name='id' direction='in'/>
    </method>

    <method name='GetCapabilities'>
      <arg type='as' direction='out'/>
    </method>

    <method name='GetServerInformation'>
      <arg type='s' name='name' direction='out'/>
      <arg type='s' name='vendor' direction='out'/>
      <arg type='s' name='version' direction='out'/>
      <arg type='s' name='spec_version' direction='out'/>
    </method>

    <signal name='NotificationClosed'>
      <arg type='u' name='id'/>
      <arg type='u' name='reason'/>
    </signal>

    <signal name='ActionInvoked'>
      <arg type='u' name='id'/>
      <arg type='s' name='action_key'/>
    </signal>
  </interface>
</node>
";

#[derive(Default)]
pub struct NotificationsInner {
    next_id: Cell<NotificationId>,
    notifications: RefCell<HashMap<NotificationId, Rc<Notification>>>,
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
                Signal::builder(
                    "notification-received",
                    &[NotificationId::static_type().into()],
                    glib::Type::UNIT.into(),
                )
                .build(),
                Signal::builder(
                    "notification-closed",
                    &[NotificationId::static_type().into()],
                    glib::Type::UNIT.into(),
                )
                .build(),
            ]
        });
        SIGNALS.as_ref()
    }
}

glib::wrapper! {
    pub struct Notifications(ObjectSubclass<NotificationsInner>);
}

// XXX hack: https://github.com/gtk-rs/gtk-rs-core/issues/263
unsafe impl Send for Notifications {}
unsafe impl Sync for Notifications {}

struct Hints(HashMap<String, glib::Variant>);

#[allow(dead_code)]
impl Hints {
    fn prop<T: glib::FromVariant>(&self, name: &str) -> Option<T> {
        self.0.get(name)?.get()
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
            if let Some(v) = v.get::<String>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<i32>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<bool>() {
                s.field(k, &v);
            } else if let Some(v) = v.get::<u8>() {
                s.field(k, &v);
            } else {
                s.field(k, v);
            };
        }
        s.finish()
    }
}

impl glib::StaticVariantType for Hints {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        glib::VariantTy::new("a{sv}").unwrap().into()
    }
}

impl glib::FromVariant for Hints {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        variant.get().map(Self)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, glib::GBoxed, PartialEq, Eq)]
#[gboxed(type_name = "S76NotificationId")]
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

impl Notifications {
    pub fn new() -> Self {
        let notifications = glib::Object::new::<Self>(&[]).unwrap();

        gio::bus_own_name(
            gio::BusType::Session,
            "org.freedesktop.Notifications",
            gio::BusNameOwnerFlags::NONE,
            clone!(@strong notifications => move |connection, name| notifications.bus_acquired(connection, name)),
            clone!(@strong notifications => move |connection, name| notifications.name_acquired(connection, name)),
            clone!(@strong notifications => move |connection, name| notifications.name_lost(connection, name)),
        );

        notifications
    }

    fn inner(&self) -> &NotificationsInner {
        NotificationsInner::from_instance(self)
    }

    fn next_id(&self) -> NotificationId {
        let next_id = &self.inner().next_id;
        let id = next_id.get();
        next_id.set(NotificationId::new(u32::from(id).wrapping_add(1)).unwrap_or_default());
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
        expire_timeout: i32,
    ) -> NotificationId {
        let id = replaces_id.unwrap_or_else(|| self.next_id());

        let notification = Rc::new(Notification {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions,
            hints,
        });

        self.inner()
            .notifications
            .borrow_mut()
            .insert(id, notification);

        if expire_timeout != 0 {
            let expire_timeout = if expire_timeout < 0 {
                1000 // XXX
            } else {
                expire_timeout as u64
            };
            let expire_timeout = Duration::from_millis(expire_timeout);
            glib::timeout_add_local(
                expire_timeout,
                clone!(@strong self as self_ => move || {
                    self_.close_notification(id);
                    Continue(false)
                }),
            );
        }

        // XXX
        self.emit_by_name("notification-received", &[&id]).unwrap();

        id
    }

    fn close_notification(&self, id: NotificationId) {
        self.emit_by_name("notification-closed", &[&id]).unwrap();
    }

    fn bus_acquired(&self, _connection: gio::DBusConnection, _name: &str) {}

    fn name_acquired(&self, connection: gio::DBusConnection, _name: &str) {
        let introspection_data = gio::DBusNodeInfo::for_xml(NOTIFICATIONS_XML).unwrap();
        let interface_info = introspection_data
            .lookup_interface("org.freedesktop.Notifications")
            .unwrap();
        let method_call = clone!(@strong self as self_ => move |_connection: gio::DBusConnection,
                           _sender: &str,
                           _path: &str,
                           _interface: &str,
                           method: &str,
                           args: glib::Variant,
                           invocation: gio::DBusMethodInvocation| {
            match method {
                "Notify" => {
                    let (app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) = args.get().unwrap();
                    let replaces_id = NotificationId::new(replaces_id);
                    let res = self_.handle_notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout);
                    invocation.return_value(Some(&(u32::from(res),).to_variant()));
                    // TODO error?
                }
                "CloseNotification" => {
                    let (id,) = args.get::<(u32,)>().unwrap();
                    if let Some(id) = NotificationId::new(id) {
                        self_.close_notification(id);
                    }
                    invocation.return_value(None);
                    // TODO error?
                }
                "GetCapabilities" => {
                    // TODO: body-markup, sound
                    let capabilities = vec!["actions", "body", "icon-static", "persistence"];
                    invocation.return_value(Some(&(capabilities,).to_variant()));
                }
                "GetServerInformation" => {
                    let information = ("cosmic-panel", "system76", env!("CARGO_PKG_VERSION"), "1.2");
                    invocation.return_value(Some(&information.to_variant()));
                }
                _ => unreachable!()
            }
        });
        let get_property = |_: gio::DBusConnection,
                            _sender: &str,
                            _path: &str,
                            _interface: &str,
                            _prop: &str| { unreachable!() };
        let set_property = |_: gio::DBusConnection,
                            _sender: &str,
                            _path: &str,
                            _interface: &str,
                            _prop: &str,
                            _value: glib::Variant| { unreachable!() };
        if let Err(err) = connection.register_object(
            "/org/freedesktop/Notifications",
            &interface_info,
            method_call,
            get_property,
            set_property,
        ) {
            eprintln!("Failed to register object: {}", err);
        }
    }

    fn name_lost(&self, _connection: Option<gio::DBusConnection>, _name: &str) {}

    pub fn get(&self, id: NotificationId) -> Option<Rc<Notification>> {
        self.inner().notifications.borrow().get(&id).cloned()
    }

    pub fn connect_notification_recieved<F: Fn(NotificationId) + 'static>(
        &self,
        cb: F,
    ) -> SignalHandlerId {
        self.connect_local("notification-received", false, move |values| {
            let id = values[1].get().unwrap();
            cb(id);
            None
        })
        .unwrap()
    }
}
