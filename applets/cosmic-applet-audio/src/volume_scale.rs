// TODO: Use `Volume::ui_max()`?
// * Make sure volumes greater than this are handled properly.

use gtk4::{glib, prelude::*, subclass::prelude::*};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};

use crate::PA;

#[derive(Default)]
pub struct VolumeScaleImp {
    name: Rc<RefCell<Option<String>>>,
    in_drag: Cell<bool>,
    volume_to_set: Cell<Option<f64>>,
}

#[glib::object_subclass]
impl ObjectSubclass for VolumeScaleImp {
    const NAME: &'static str = "VolumeScale";
    type Type = VolumeScale;
    type ParentType = gtk4::Scale;
}

impl ObjectImpl for VolumeScaleImp {
    fn constructed(&self, obj: &Self::Type) {
        obj.set_range(0., 100.);

        let gesture_drag = gtk4::GestureDrag::new();
        gesture_drag.connect_drag_begin(glib::clone!(@weak obj => move |_, _, _| {
            obj.imp().in_drag.set(true);
        }));
        gesture_drag.connect_drag_end(glib::clone!(@weak obj => move |_, _, _| {
            obj.imp().in_drag.set(false);
            if let Some(volume) = obj.imp().volume_to_set.take() {
                obj.set_value(volume);
            }
        }));
        obj.add_controller(&gesture_drag);
    }
}

impl WidgetImpl for VolumeScaleImp {}
impl RangeImpl for VolumeScaleImp {}
impl ScaleImpl for VolumeScaleImp {}

glib::wrapper! {
    pub struct VolumeScale(ObjectSubclass<VolumeScaleImp>)
        @extends gtk4::Scale, gtk4::Range, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Orientable;
}

impl VolumeScale {
    pub fn new(pa: PA, sink: bool) -> Self {
        let scale: VolumeScale = glib::Object::new(&[]).unwrap();
        let name = scale.imp().name.clone();
        let updater = Updater::new(move |value: f64| {
            let name = name.clone();
            let pa = pa.clone();
            async move {
                let mut volumes = ChannelVolumes::default();
                let volume = value * (Volume::NORMAL.0 as f64) / 100.;
                volumes.set(1, Volume(volume as _)); // XXX ?

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
        scale
    }

    pub fn set_volume(&self, volume: &ChannelVolumes) {
        let value = volume.avg().0 as f64 / (Volume::NORMAL.0 as f64) * 100.;
        if self.imp().in_drag.get() {
            // Don't set value of scale while it is being moved
            self.imp().volume_to_set.set(Some(value));
        } else {
            self.set_value(value);
        }
    }

    pub fn set_name(&self, name: Option<String>) {
        *self.imp().name.borrow_mut() = name;
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
