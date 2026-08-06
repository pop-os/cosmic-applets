// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cctk::{
    sctk::reexports::calloop, toplevel_info::ToplevelInfo,
    wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
};
use cosmic::iced::{Subscription, futures, stream};
use futures::SinkExt;

use crate::wayland_handler::wayland_handler;

pub fn wayland_subscription() -> Subscription<WaylandUpdate> {
    Subscription::run_with(std::any::TypeId::of::<WaylandUpdate>(), |_| {
        stream::channel(
            50,
            |mut output: futures::channel::mpsc::Sender<WaylandUpdate>| async move {
                let (request_tx, request_rx) = calloop::channel::channel();
                let (update_tx, mut update_rx) = futures::channel::mpsc::unbounded();

                std::thread::spawn(move || wayland_handler(update_tx, request_rx));
                let _ = output.send(WaylandUpdate::Init(request_tx)).await;

                use futures::StreamExt;
                while let Some(update) = update_rx.next().await {
                    if output.send(update).await.is_err() {
                        return;
                    }
                }
                let _ = output.send(WaylandUpdate::Finished).await;
            },
        )
    })
}

#[derive(Clone, Debug)]
pub enum WaylandUpdate {
    Init(calloop::channel::Sender<WaylandRequest>),
    Finished,
    Toplevel(Box<ToplevelUpdate>),
}

#[derive(Clone, Debug)]
pub enum ToplevelUpdate {
    Add(ToplevelInfo),
    Update(ToplevelInfo),
    Remove(ExtForeignToplevelHandleV1),
}

#[derive(Clone, Debug)]
pub enum WaylandRequest {
    Activate(ExtForeignToplevelHandleV1),
}
