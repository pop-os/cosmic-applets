use zbus::{dbus_interface, Result, SignalContext, zvariant::{OwnedValue, Type}};
use cosmic::iced_native::subscription::{self, Subscription};

use std::{ num::NonZeroU32,
          collections::HashMap,
          sync::atomic::{AtomicU32, Ordering},
};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};

static ID: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone)]
pub struct Notification {
  id: NonZeroU32,
  app_name: String,
  app_icon: String,
  summary: String,
  body: String,
  actions: Vec<String>,
  hints: Hints,
  expire_timeout: i32,
}

#[derive(Clone, Debug)]
pub enum Message {
  Notify(Notification),
  CloseNotification(u32),
}

#[derive(Clone, Debug)]
struct Notifications(mpsc::Sender<Message>);

/*
impl Notifications {
  async fn send(&mut self, msg: Message) -> Result<()> {
    self.0.send(msg).await.map_err(|e| zbus::Error::Unsupported)
  }
}
*/

#[dbus_interface(name = "org.freedesktop.Notifications")]
#[allow(dead_code, non_snake_case)]
impl Notifications {
    fn Notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: Hints,
        expire_timeout: i32,
    ) -> u32 {
      let id = NonZeroU32::new(if replaces_id != 0 { replaces_id } else { let id = ID.load(Ordering::Relaxed).wrapping_add(1); ID.store(id, Ordering::Relaxed); id }).unwrap_or(NonZeroU32::new(1).unwrap());
      let _ = self.0.send(Message::Notify(Notification {
        id,
        app_name,
        app_icon,
        summary,
        body,
        actions,
        hints,
        expire_timeout,
      }));
      id.into()
    }

    async fn CloseNotification(&self, id: u32) {
      let _ = self.0.send(Message::CloseNotification(id));
      // TODO
      /*
        if let Some(id) = NotificationId::new(id) {
            self.0
                .sender
                .unbounded_send(Event::CloseNotification(id))
                .unwrap();
        }
        // TODO error?
        // */
    }

    fn GetCapabilities(&self) -> Vec<&'static str> {
        // TODO: body-markup, sound
        vec!["actions", "body", "icon-static", "persistence"]
    }

    fn GetServerInformation(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("cosmic-panel", "system76", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[dbus_interface(signal)]
    async fn NotificationClosed(ctxt: &SignalContext<'_>, id: u32, reason: u32) -> Result<()>;

    #[dbus_interface(signal)]
    async fn ActionInvoked(ctxt: &SignalContext<'_>, id: u32, action_key: &str) -> Result<()>;
}

#[derive(Type, Deserialize, Serialize, PartialEq, Clone)]
struct Hints(HashMap<String, OwnedValue>);

#[allow(dead_code)]
impl Hints {
    fn prop<T: TryFrom<OwnedValue>>(&self, name: &str) -> Option<T> {
        T::try_from(self.0.get(name)?.clone()).ok()
    }

    fn actions_icon(&self) -> bool {
        self.prop("actions-icon").unwrap_or(false)
    }

    fn category(&self) -> Option<String> {
        self.prop("category")
    }

    fn desktop_entry(&self) -> Option<String> {
        self.prop("desktop-entry")
    }

    fn image_data(&self) -> Option<(i32, i32, i32, bool, i32, i32, Vec<u8>)> {
        self.prop("image-data")
            .or_else(|| self.prop("image_data"))
            .or_else(|| self.prop("icon_data"))
    }

    fn image_path(&self) -> Option<String> {
        self.prop("image-path").or_else(|| self.prop("image_path"))
    }

    fn resident(&self) -> bool {
        self.prop("resident").unwrap_or(false)
    }

    fn sound_file(&self) -> Option<String> {
        self.prop("sound-file")
    }

    fn sound_name(&self) -> Option<String> {
        self.prop("sound-name")
    }

    fn transient(&self) -> bool {
        self.prop("transient").unwrap_or(false)
    }

    fn xy(&self) -> Option<(u8, u8)> {
        Some((self.prop("x")?, self.prop("y")?))
    }

    fn urgency(&self) -> Option<u8> {
        self.prop("urgency")
    }
}

impl std::fmt::Debug for Hints {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut s = f.debug_struct("Hints");
        for (k, v) in &self.0 {
            if let Ok(v) = <&str>::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = i32::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = bool::try_from(v) {
                s.field(k, &v);
            } else if let Ok(v) = u8::try_from(v) {
                s.field(k, &v);
            } else {
                s.field(k, v);
            };
        }
        s.finish()
    }
}

enum State {
  Connected,
  Disconnected,
}

pub fn connect() -> Subscription<Message> {
  struct Connect;

  subscription::unfold(std::any::TypeId::of::<Connect>(), State::Disconnected, |state| async move {
    match state {
      State::Connected => (None, State::Connected),
      State::Disconnected => (None, State::Disconnected),
    }
  })
}
