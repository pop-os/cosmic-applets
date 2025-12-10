// Copyright 2026 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

// Based on https://github.com/pop-os/cosmic-comp/blob/master/src/dbus/name_owners.rs,
// but only tracking unique names, and using tokio executor.

use futures::StreamExt;
use std::{
    collections::HashSet,
    future::{Future, poll_fn},
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll, Waker},
};
use zbus::{
    fdo,
    names::{BusName, UniqueName},
};

#[derive(Debug)]
struct Inner {
    unique_names: HashSet<UniqueName<'static>>,
    stream: fdo::NameOwnerChangedStream,
    // Waker from `update_task` is stored, so that task will still be woken after
    // polling elsewhere.
    waker: Waker,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Wake `update_task` so it can terminate
        self.waker.wake_by_ref();
    }
}

impl Inner {
    /// Process all events so far on `stream`, and update `unique_names`.
    fn update_if_needed(&mut self) {
        let mut context = Context::from_waker(&self.waker);
        while let Poll::Ready(val) = self.stream.poll_next_unpin(&mut context) {
            let val = val.unwrap();
            let args = val.args().unwrap();
            match args.name {
                BusName::Unique(name) => {
                    if args.new_owner.is_some() {
                        self.unique_names.insert(name.to_owned());
                    } else {
                        self.unique_names.remove(&name.to_owned());
                    }
                }
                BusName::WellKnown(_) => {}
            }
        }
    }
}

/// This task polls the steam regularly, to make sure events on the stream aren't just
/// buffered indefinitely.
fn update_task(inner: Weak<Mutex<Inner>>) -> impl Future<Output = ()> {
    poll_fn(move |context| {
        if let Some(inner) = inner.upgrade() {
            let mut inner = inner.lock().unwrap();
            inner.waker = context.waker().clone();
            inner.update_if_needed();
            // Nothing to do now until waker is invoked
            Poll::Pending
        } else {
            // All strong references have been dropped, so task has nothing left to do.
            Poll::Ready(())
        }
    })
}

#[derive(Clone, Debug)]
pub struct UniqueNames(Arc<Mutex<Inner>>);

impl UniqueNames {
    pub async fn new(connection: &zbus::Connection) -> zbus::Result<Self> {
        let dbus = fdo::DBusProxy::new(connection).await?;
        let stream = dbus.receive_name_owner_changed().await?;

        let names = dbus.list_names().await?;
        let unique_names = names
            .iter()
            .filter_map(|n| match n.inner() {
                BusName::Unique(name) => Some(name.to_owned()),
                BusName::WellKnown(_) => None,
            })
            .collect();

        let inner = Arc::new(Mutex::new(Inner {
            unique_names,
            stream,
            waker: Waker::noop().clone(),
        }));

        tokio::spawn(update_task(Arc::downgrade(&inner)));

        Ok(UniqueNames(inner))
    }

    #[allow(dead_code)]
    pub fn has_unique_name(&self, name: &UniqueName<'_>) -> bool {
        let mut inner = self.0.lock().unwrap();
        inner.update_if_needed();
        inner.unique_names.contains(name)
    }
}
