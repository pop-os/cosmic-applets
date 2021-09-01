use gtk4::{
    gio,
    glib::{self, clone},
    prelude::*,
};
use std::sync::{Arc, Mutex};

static STATUS_NOTIFIER_XML: &str = "
<node name='/StatusNotifierWatcher'>
  <interface name='org.kde.StatusNotifierWatcher'>
    <method name='RegisterStatusNotifierItem'>
      <arg name='service' type='s' direction='in' />
    </method>

    <method name='RegisterStatusNotifierHost'>
      <arg name='service' type='s' direction='in' />
    </method>

    <property name='RegisteredStatusNotifierItems' type='as' access='read' />
    <property name='IsStatusNotifierHostRegistered' type='b' access='read' />
    <property name='ProtocolVersion' type='i' access='read' />

    <signal name='StatusNotifierItemRegistered'>
      <arg type='s' name='service' direction='out' />
    </signal>

    <signal name='StatusNotifierItemUnregistered'>
      <arg type='s' name='service' direction='out' />
    </signal>

    <signal name='StatusNotifierHostRegistered' />

    <signal name='StatusNotifierHostUnregistered' />
  </interface>
</node>
";

pub fn start() {
    // XXX flags?
    gio::bus_own_name(
        gio::BusType::Session,
        "org.kde.StatusNotifierWatcher",
        gio::BusNameOwnerFlags::NONE,
        bus_acquired,
        name_acquired,
        name_lost,
    );
}

fn bus_acquired(_connection: gio::DBusConnection, _name: &str) {}

fn name_acquired(connection: gio::DBusConnection, _name: &str) {
    let introspection_data = gio::DBusNodeInfo::for_xml(STATUS_NOTIFIER_XML).unwrap();
    let interface_info = introspection_data
        .lookup_interface("org.kde.StatusNotifierWatcher")
        .unwrap();
    let items = Arc::new(Mutex::new(Vec::<String>::new()));
    let method_call = clone!(@strong items => move |connection: gio::DBusConnection,
                       sender: &str,
                       path: &str,
                       interface: &str,
                       method: &str,
                       args: glib::Variant,
                       invocation: gio::DBusMethodInvocation| {
        match method {
            "RegisterStatusNotifierItem" => {
                let (service,) = args.get::<(String,)>().unwrap();
                let service = format!("{}{}", sender, service);
                connection.emit_signal(None, path, interface, "StatusNotifierItemRegistered", Some(&(&service,).to_variant())).unwrap();
                // XXX emit unreigstered
                items.lock().unwrap().push(service);
            }
            "RegisterStatusNotifierHost" => {
                let (_service,) = args.get::<(String,)>().unwrap();
                // XXX emit registed/unregistered
            }
            _ => unreachable!()
        }
        invocation.return_dbus_error("DBus.Error.UnknownMethod", "Unknown method");
    });
    let get_property = clone!(@strong items => move |_: gio::DBusConnection, _sender: &str, _path: &str, _interface: &str, prop: &str| {
        match prop {
            "RegisteredStatusNotifierItems" => items.lock().unwrap().to_variant(),
            "IsStatusNotifierHostRegistered" => true.to_variant(),
            "ProtocolVersion" => 0i32.to_variant(),
            _ => unreachable!(),
        }
    });
    let set_property = |_: gio::DBusConnection,
                        _sender: &str,
                        _path: &str,
                        _interface: &str,
                        _prop: &str,
                        _value: glib::Variant| { unreachable!() };
    if let Err(err) = connection.register_object(
        "/StatusNotifierWatcher",
        &interface_info,
        method_call,
        get_property,
        set_property,
    ) {
        eprintln!("Failed to register object: {}", err);
    }
}
fn name_lost(_connection: Option<gio::DBusConnection>, _name: &str) {}
