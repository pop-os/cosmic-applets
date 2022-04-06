// SPDX-License-Identifier: LGPL-3.0-or-later
use std::future::Future;
use tokio::sync::oneshot;

pub fn spawn<O, F>(future: F) -> tokio::task::JoinHandle<O>
where
    F: Future<Output = O> + Send + 'static,
    O: Send + 'static,
{
    crate::RT.spawn(future)
}

pub fn block_on<O, F>(future: F) -> O
where
    F: Future<Output = O> + Send + 'static,
    O: Send + 'static,
{
    crate::RT.block_on(future)
}

pub fn spawn_local<F: Future<Output = ()> + 'static>(future: F) {
    gtk4::glib::MainContext::default().spawn_local(future);
}

pub async fn wait_for_local<O, F>(future: F) -> Option<O>
where
    O: Send + 'static,
    F: Future<Output = O> + Send + 'static,
{
    let (tx, rx) = oneshot::channel::<O>();
    gtk4::glib::MainContext::default().spawn_local(async move {
        std::mem::drop(tx.send(future.await));
    });
    rx.await.ok()
}
