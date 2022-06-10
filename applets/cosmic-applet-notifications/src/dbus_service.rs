use futures::prelude::*;
use gtk4::glib::{self, clone};
use std::cell::Cell;
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};
use zbus_names::WellKnownName;

pub async fn create<
    F: Fn(zbus::ConnectionBuilder<'static>) -> zbus::Result<zbus::ConnectionBuilder<'static>>,
>(
    well_known_name: &'static str,
    serve_cb: F,
) -> zbus::Result<zbus::Connection> {
    let well_known_name = WellKnownName::try_from(well_known_name)?;

    let connection = serve_cb(zbus::ConnectionBuilder::session()?)?
        .build()
        .await?;
    let dbus_proxy = DBusProxy::new(&connection).await?;
    let mut name_owner_changed_stream = dbus_proxy.receive_name_owner_changed().await?;

    let flags = RequestNameFlags::AllowReplacement.into();
    match dbus_proxy
        .request_name(well_known_name.as_ref(), flags)
        .await?
    {
        RequestNameReply::InQueue => {
            eprintln!("Bus name '{}' already owned", well_known_name);
        }
        _ => {}
    }

    glib::MainContext::default().spawn_local(clone!(@strong connection => async move {
        let have_bus_name = Cell::new(false);
        let unique_name = connection.unique_name().map(|x| x.as_ref());
        while let Some(evt) = name_owner_changed_stream.next().await {
            let args = match evt.args() {
                Ok(args) => args,
                Err(_) => { continue; },
            };
            if args.name.as_ref() == well_known_name {
                if args.new_owner.as_ref() == unique_name.as_ref() {
                    eprintln!("Acquired bus name: {}", well_known_name);
                    have_bus_name.set(true);
                } else if have_bus_name.get() {
                    eprintln!("Lost bus name: {}", well_known_name);
                    have_bus_name.set(false);
                }
            }
        }
    }));

    Ok(connection)
}
