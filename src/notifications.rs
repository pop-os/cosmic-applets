use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
};
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt,
    num::NonZeroU32,
    sync::{Arc, Mutex},
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

pub struct Notifications {
    next_id: Mutex<NonZeroU32>,
}

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

impl Notifications {
    pub fn new() -> Arc<Self> {
        let notifications = Arc::new(Notifications {
            next_id: Mutex::new(NonZeroU32::new(1).unwrap()),
        });

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

    fn next_id(&self) -> NonZeroU32 {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id = NonZeroU32::new(u32::from(*next_id).wrapping_add(1))
            .unwrap_or(NonZeroU32::new(1).unwrap());
        id
    }

    fn notify(
        &self,
        app_name: String,
        replaces_id: Option<NonZeroU32>,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: Hints,
        expire_timeout: i32,
    ) -> NonZeroU32 {
        let id = replaces_id.unwrap_or_else(|| self.next_id());

        println!(
            "{:?}",
            (
                id,
                app_name,
                app_icon,
                summary,
                body,
                actions,
                hints,
                expire_timeout
            )
        );

        // TODO

        id
    }

    fn close_notification(&self, _id: u32) {}

    fn bus_acquired(self: &Arc<Self>, _connection: gio::DBusConnection, _name: &str) {}

    fn name_acquired(self: &Arc<Self>, connection: gio::DBusConnection, _name: &str) {
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
                    let replaces_id = NonZeroU32::new(replaces_id);
                    let res = self_.notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout);
                    invocation.return_value(Some(&(u32::from(res),).to_variant()));
                    // TODO error?
                }
                "CloseNotification" => {
                    let (id,) = args.get::<(u32,)>().unwrap();
                    self_.close_notification(id);
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

    fn name_lost(self: &Arc<Self>, _connection: Option<gio::DBusConnection>, _name: &str) {}
}
