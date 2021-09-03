use cascade::cascade;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::deref_cell::DerefCell;

/// Unlike gtk4's `MenuButton`, this supports a custom child.
#[derive(Default)]
pub struct PopoverContainerInner {
    child: DerefCell<gtk4::Widget>,
    popover: DerefCell<gtk4::Popover>,
}

#[glib::object_subclass]
impl ObjectSubclass for PopoverContainerInner {
    const NAME: &'static str = "S76PopoverContainer";
    type ParentType = gtk4::Widget;
    type Type = PopoverContainer;
}

impl ObjectImpl for PopoverContainerInner {
    fn constructed(&self, obj: &PopoverContainer) {
        let popover = cascade! {
            gtk4::Popover::new();
            ..set_parent(obj);
        };

        self.popover.set(popover);
    }

    fn dispose(&self, _obj: &PopoverContainer) {
        self.child.unparent();
        self.popover.unparent();
    }
}

impl WidgetImpl for PopoverContainerInner {
    fn measure(
        &self,
        _obj: &PopoverContainer,
        orientation: gtk4::Orientation,
        for_size: i32,
    ) -> (i32, i32, i32, i32) {
        self.child.measure(orientation, for_size)
    }

    fn size_allocate(&self, _obj: &PopoverContainer, width: i32, height: i32, baseline: i32) {
        self.child.size_allocate(
            &gtk4::Allocation {
                x: 0,
                y: 0,
                width,
                height,
            },
            baseline,
        );
        self.popover.present();
    }

    fn focus(&self, _obj: &PopoverContainer, direction: gtk4::DirectionType) -> bool {
        if self.popover.is_visible() {
            self.popover.child_focus(direction)
        } else {
            self.child.child_focus(direction)
        }
    }
}

glib::wrapper! {
    pub struct PopoverContainer(ObjectSubclass<PopoverContainerInner>)
        @extends gtk4::Widget;
}

impl PopoverContainer {
    pub fn new<T: IsA<gtk4::Widget>>(child: &T) -> Self {
        let obj = glib::Object::new::<Self>(&[]).unwrap();
        child.set_parent(&obj);
        obj.inner().child.set(child.clone().upcast());
        obj
    }

    fn inner(&self) -> &PopoverContainerInner {
        PopoverContainerInner::from_instance(self)
    }

    pub fn popover(&self) -> &gtk4::Popover {
        &self.inner().popover
    }

    pub fn popup(&self) {
        self.popover().popup();
    }

    pub fn popdown(&self) {
        self.popover().popdown();
    }
}
