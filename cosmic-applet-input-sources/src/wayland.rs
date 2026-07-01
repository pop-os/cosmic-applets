use cctk::{
    cosmic_protocols::keyboard_layout::v1::client::zcosmic_keyboard_layout_v1::ZcosmicKeyboardLayoutV1,
    keyboard_layout::{KeyboardLayoutHandler, KeyboardLayoutState},
    sctk::{
        self,
        registry::{ProvidesRegistryState, RegistryState},
        seat::{Capability, SeatHandler, SeatState},
    },
    wayland_client::{
        Connection, QueueHandle, delegate_noop,
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_seat},
    },
};
use cosmic::iced;
use futures::{SinkExt, channel::mpsc, executor::block_on};
use std::{hash::Hash, thread};

#[derive(Clone, Debug)]
pub enum Event {
    KeyboardLayout(ZcosmicKeyboardLayoutV1),
    Group(usize),
}

pub fn subscription(connection: Connection) -> iced::Subscription<Event> {
    #[derive(Clone)]
    struct WaylandSubscription(Connection);
    impl Hash for WaylandSubscription {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.backend().display_id().hash(state);
        }
    }
    iced::Subscription::run_with(
        WaylandSubscription(connection),
        |WaylandSubscription(connection)| {
            let connection = connection.clone();
            iced::stream::channel(8, move |sender| async move {
                thread::spawn(move || thread(connection, sender));
            })
        },
    )
}

struct Keyboard {
    seat: wl_seat::WlSeat,
    keyboard_layout: ZcosmicKeyboardLayoutV1,
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        self.keyboard_layout.destroy();
    }
}

struct AppData {
    seat_state: SeatState,
    registry_state: RegistryState,
    keyboard_layout_state: KeyboardLayoutState,
    keyboard: Option<Keyboard>,
    sender: mpsc::Sender<Event>,
}

impl KeyboardLayoutHandler for AppData {
    fn group(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _keyboard_layout: &ZcosmicKeyboardLayoutV1,
        group: u32,
    ) {
        let _ = block_on(self.sender.send(Event::Group(group as usize)));
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers![SeatState,];
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = seat.get_keyboard(qh, ());
            let keyboard_layout = self
                .keyboard_layout_state
                .get_keyboard_layout(&keyboard, qh);
            keyboard.release();

            if let Some(keyboard_layout) = keyboard_layout {
                let _ = block_on(
                    self.sender
                        .send(Event::KeyboardLayout(keyboard_layout.clone())),
                );
                self.keyboard = Some(Keyboard {
                    seat,
                    keyboard_layout,
                });
            } else {
                keyboard.release();
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        _capability: Capability,
    ) {
        self.keyboard.take_if(|x| x.seat == seat);
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.keyboard.take_if(|x| x.seat == seat);
    }
}

fn thread(conn: Connection, sender: mpsc::Sender<Event>) {
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let keyboard_layout_state = KeyboardLayoutState::new(&registry_state, &qh);

    let mut app_data = AppData {
        seat_state,
        registry_state,
        keyboard_layout_state,
        keyboard: None,
        sender,
    };
    loop {
        event_queue.blocking_dispatch(&mut app_data).unwrap();
    }
}

sctk::delegate_registry!(AppData);
sctk::delegate_seat!(AppData);
cctk::delegate_keyboard_layout!(AppData);
delegate_noop!(AppData: ignore wl_keyboard::WlKeyboard);
