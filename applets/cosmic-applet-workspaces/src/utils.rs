// SPDX-License-Identifier: MPL-2.0-only

use std::path::PathBuf;

use gtk4::glib;
use std::future::Future;

#[derive(Debug)]
pub enum Event {
    WorkspaceList,
    Activate(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub(crate) id: u32,
    pub(crate) active: bool,
}

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedWorkspace")]
pub struct BoxedWorkspace(pub Option<Workspace>);

#[derive(Clone, Debug, Default, glib::Boxed)]
#[boxed_type(name = "BoxedWorkspaceList")]
pub struct BoxedWorkspaceList(pub Vec<Workspace>);

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
