use gdk4_x11::x11::xlib;
use glib::translate::ToGlibPtr;
use gtk4::{glib, prelude::*};
use std::{
    ffi::{CString, NulError},
    os::raw::{c_int, c_long},
};

pub use std::os::raw::{c_uchar, c_ulong, c_ushort};

pub fn get_window_x11(
    window: &gtk4::Window,
) -> Option<(gdk4_x11::X11Display, gdk4_x11::X11Surface)> {
    let surface = window.surface()?.downcast::<gdk4_x11::X11Surface>().ok()?;
    let display = surface.display()?.downcast::<gdk4_x11::X11Display>().ok()?;
    Some((display, surface))
}

#[repr(transparent)]
pub struct Atom(xlib::Atom);

impl Atom {
    pub fn new(display: &gdk4_x11::X11Display, prop: &str) -> Result<Self, NulError> {
        unsafe {
            let prop = CString::new(prop)?;
            Ok(Self(gdk4_x11::ffi::gdk_x11_get_xatom_by_name_for_display(
                display.to_glib_none().0,
                prop.as_ptr(),
            )))
        }
    }
}

pub unsafe trait XElement {
    const TYPE: xlib::Atom;
    const SIZE: c_int;
}

unsafe impl XElement for c_uchar {
    const TYPE: xlib::Atom = xlib::XA_CARDINAL;
    const SIZE: c_int = 8;
}

unsafe impl XElement for c_ushort {
    const TYPE: xlib::Atom = xlib::XA_CARDINAL;
    const SIZE: c_int = 16;
}

unsafe impl XElement for c_ulong {
    const TYPE: xlib::Atom = xlib::XA_CARDINAL;
    const SIZE: c_int = 32;
}

unsafe impl XElement for Atom {
    const TYPE: xlib::Atom = xlib::XA_ATOM;
    const SIZE: c_int = 32;
}

pub unsafe trait XProp {
    const TYPE: xlib::Atom;
    const SIZE: c_int;

    fn ptr(&self) -> *const u8;

    fn nelements(&self) -> c_int;
}

unsafe impl<T: XElement, const LEN: usize> XProp for &[T; LEN] {
    const TYPE: xlib::Atom = T::TYPE;
    const SIZE: c_int = T::SIZE;

    fn ptr(&self) -> *const u8 {
        self.as_ptr() as _
    }

    fn nelements(&self) -> c_int {
        LEN as c_int
    }
}

unsafe impl<T: XElement> XProp for &[T] {
    const TYPE: xlib::Atom = T::TYPE;
    const SIZE: c_int = T::SIZE;

    fn ptr(&self) -> *const u8 {
        self.as_ptr() as _
    }

    fn nelements(&self) -> c_int {
        self.len() as c_int
    }
}

unsafe impl XProp for &str {
    const TYPE: xlib::Atom = xlib::XA_STRING;
    const SIZE: c_int = 8;

    fn ptr(&self) -> *const u8 {
        self.as_ptr()
    }

    fn nelements(&self) -> c_int {
        self.len() as c_int
    }
}

#[allow(dead_code)]
pub enum PropMode {
    Replace,
    Prepend,
    Append,
}

pub unsafe fn change_property<T: XProp>(
    display: &gdk4_x11::X11Display,
    surface: &gdk4_x11::X11Surface,
    prop: &str,
    mode: PropMode,
    value: T,
) {
    // TODO check error return value
    let mode = match mode {
        PropMode::Replace => xlib::PropModeReplace,
        PropMode::Prepend => xlib::PropModePrepend,
        PropMode::Append => xlib::PropModeAppend,
    };
    xlib::XChangeProperty(
        display.xdisplay(),
        surface.xid(),
        Atom::new(display, prop).unwrap().0,
        T::TYPE,
        T::SIZE,
        mode,
        value.ptr(),
        value.nelements(),
    );
}

pub unsafe fn set_position(
    display: &gdk4_x11::X11Display,
    surface: &gdk4_x11::X11Surface,
    x: c_int,
    y: c_int,
) {
    // XXX check error return value
    let hints = xlib::XAllocSizeHints();
    let mut supplied: c_long = 0;
    xlib::XGetWMNormalHints(display.xdisplay(), surface.xid(), hints, &mut supplied);
    (*hints).x = x;
    (*hints).y = y;
    xlib::XSetWMNormalHints(display.xdisplay(), surface.xid(), hints);
    xlib::XFree(hints as *mut _);
}
