use cctk::{
    cosmic_protocols::keyboard_layout::v1::client::zcosmic_keyboard_layout_v1::ZcosmicKeyboardLayoutV1,
    keyboard_layout::{KeyboardLayoutHandler, KeyboardLayoutState},
    sctk::{
        self,
        registry::{ProvidesRegistryState, RegistryState},
        seat::{
            Capability, SeatHandler, SeatState,
            keyboard::{KeyEvent, KeyboardHandler, Keymap, Keysym, Modifiers, RawModifiers},
        },
    },
    wayland_client::{
        Connection, QueueHandle,
        globals::registry_queue_init,
        protocol::{wl_keyboard, wl_seat, wl_surface},
    },
};
use cosmic::iced_futures;
use futures::{channel::mpsc, executor::block_on, SinkExt};
use std::thread;
use xkbcommon::xkb;

#[derive(Clone, Debug)]
pub enum Event {
    // TODO layout, description, variant?
    LayoutList(Vec<String>),
    Layout(usize),
    KeyboardLayout(ZcosmicKeyboardLayoutV1),
}

pub fn subscription() -> iced_futures::Subscription<Event> {
    iced_futures::Subscription::run(|| {
        iced_futures::stream::channel(8, |sender| async {
            thread::spawn(|| thread(sender));
        })
    })
}

struct AppData {
    seat_state: SeatState,
    registry_state: RegistryState,
    keyboard_layout_state: KeyboardLayoutState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keymap: Option<xkb::Keymap>,
    sender: mpsc::Sender<Event>,
}

impl KeyboardLayoutHandler for AppData {
    fn group(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        keyboard_layout: &ZcosmicKeyboardLayoutV1,
        group: u32,
    ) {
        block_on(self.sender.send(Event::Layout(group as usize)));
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
        if capability == Capability::Keyboard {
            // TODO multiple seats?
            let keyboard = self.seat_state.get_keyboard(qh, &seat, None).unwrap();
            let keyboard_layout = self.keyboard_layout_state.get_keyboard_layout(&keyboard, qh);
            self.keyboard = Some(keyboard);

            if let Some(keyboard_layout) = keyboard_layout {
                block_on(self.sender.send(Event::KeyboardLayout(keyboard_layout)));
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _capability: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for AppData {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _keysyms: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
    }

    fn update_keymap(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        keymap: Keymap<'_>,
    ) {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_string(
            &context,
            keymap.as_string(),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap();

        block_on(self.sender.send(Event::LayoutList(keymap.layouts().map(|x| x.to_owned()).collect())));

        self.keymap = Some(keymap);
    }
}

fn thread(sender: mpsc::Sender<Event>) {
    let conn = Connection::connect_to_env().unwrap();
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let seat_state = SeatState::new(&globals, &qh);
    let keyboard_layout_state = KeyboardLayoutState::new(&registry_state, &qh);

    let mut app_data = AppData {
        seat_state,
        registry_state,
        keyboard_layout_state,
        keymap: None,
        keyboard: None,
        sender
    };
    loop {
        event_queue.blocking_dispatch(&mut app_data).unwrap();
    }
}

sctk::delegate_registry!(AppData);
sctk::delegate_seat!(AppData);
sctk::delegate_keyboard!(AppData);
cctk::delegate_keyboard_layout!(AppData);
