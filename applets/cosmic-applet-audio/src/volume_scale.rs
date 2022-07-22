use gtk4::{glib, prelude::*};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};

use crate::PA;

// Component

struct VolumeScale {
    scale: gtk4::Scale,
    name: Rc<RefCell<Option<String>>>,
}

impl VolumeScale {
    fn new(pa: PA, sink: bool) {
        let name: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0., 100., 1.);
        let updater = Updater::new(move |value: f64| {
            let name = name.clone();
            let pa = pa.clone();
            async move {
                let mut volumes = ChannelVolumes::default();
                volumes.set(0, Volume((value * 100.) as _)); // XXX ?

                let name_ref = name.borrow();
                if let Some(name) = name_ref.as_deref() {
                    if sink {
                        let fut = pa.set_sink_volume_by_name(name, &volumes);
                        drop(name_ref);
                        fut.await;
                    } else {
                        let fut = pa.set_source_volume_by_name(name, &volumes);
                        drop(name_ref);
                        fut.await;
                    }
                }
            }
        });
        scale.connect_change_value(move |_scale, _scroll, value| {
            updater.update(value);
            gtk4::Inhibit(false)
        });
    }

    fn set_value(&self, value: f64) {
        self.scale.set_value(value);
    }

    fn set_name(&self, name: Option<String>) {
        *self.name.borrow_mut() = name;
    }
}

// Perform an asynchronous update operation without queuing more than one set.
struct Updater<T: 'static> {
    updating: Rc<Cell<bool>>,
    value: Rc<Cell<Option<T>>>,
    update_fn: Rc<dyn Fn(T) -> Pin<Box<dyn Future<Output = ()> + 'static>>>,
}

impl<T: 'static> Updater<T> {
    fn new<Fut: Future<Output = ()> + 'static, F: Fn(T) -> Fut + 'static>(f: F) -> Self {
        let value = Rc::new(Cell::new(None));
        let updating = Rc::new(Cell::new(false));
        let update_fn =
            Rc::new(move |value| Box::pin(f(value)) as Pin<Box<dyn Future<Output = ()>>>);
        Self {
            updating,
            value,
            update_fn,
        }
    }

    fn update(&self, value: T) {
        self.value.set(Some(value));
        if self.updating.replace(true) == false {
            let value = self.value.clone();
            let updating = self.updating.clone();
            let update_fn = self.update_fn.clone();
            glib::MainContext::default().spawn_local(async move {
                while let Some(value) = value.take() {
                    update_fn(value).await;
                }
                updating.set(false);
            });
        }
    }
}
