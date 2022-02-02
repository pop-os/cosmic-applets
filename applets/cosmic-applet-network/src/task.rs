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

pub fn spawn_local<F: Future<Output = ()> + 'static>(future: F) {
    gtk4::glib::MainContext::default().spawn_local(future);
}

pub async fn wait_for_local<F: Future<Output = ()> + 'static>(future: F) {
    let (tx, rx) = oneshot::channel::<()>();
    gtk4::glib::MainContext::default().spawn_local(async move {
        future.await;
        let _ = tx.send(());
    });
    std::mem::drop(rx.await);
}
