// SPDX-License-Identifier: MPL-2.0-only

use std::path::PathBuf;

use gtk4::glib;
use serde::{Deserialize, Serialize};
use std::future::Future;

pub const DEST: &str = "com.System76.PopShell";
pub const PATH: &str = "/com/System76/PopShell";

#[derive(Debug)]
pub enum Event {
    WindowList,
    Activate((u32, u32)),
    Close((u32, u32)),
    Favorite((String, bool)),
    RefreshFromCache,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub(crate) entity: (u32, u32),
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) desktop_entry: String,
}

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedWindowList")]
pub struct BoxedWindowList(pub Vec<Item>);

pub fn data_path() -> PathBuf {
    let mut path = glib::user_data_dir();
    path.push(crate::ID);
    std::fs::create_dir_all(&path).expect("Could not create directory.");
    path.push("data.json");
    path
}

pub fn thread_context() -> glib::MainContext {
    glib::MainContext::thread_default().unwrap_or_else(|| {
        let ctx = glib::MainContext::new();
        ctx
    })
}

pub fn block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    let ctx = thread_context();
    ctx.with_thread_default(|| ctx.block_on(future)).unwrap()
}
