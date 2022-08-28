use iced_futures::futures;
use iced_native::subscription::{self, Subscription};
use std::sync::RwLock;
use std::thread;

extern crate libpulse_binding as pulse;
use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{Introspector, SinkInfo, SourceInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
        Context, FlagSet,
    },
    error::PAErr,
    volume::ChannelVolumes,
};
use std::sync::mpsc;
pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::unfold(
        std::any::TypeId::of::<Connect>(),
        State::Disconnected,
        |state| async move {
            match state {
                State::Disconnected => {
                    let mut from_pulse = RwLock::new(vec![]);
                    let (to_pulse, pulse_receiver) = mpsc::channel();
                    thread::spawn(move || {
                        let mainloop = pulse::mainloop::standard::Mainloop::new().unwrap();
                        let context = Context::new(&mainloop, "com.system76.cosmic.applets.audio").unwrap();
                        println!("mainloop created");
                        loop {
                            if let Ok(msg) = pulse_receiver.try_recv() {
                                match msg {
                                    Message::GetSinks => {
                                        println!("get get sinks");
                                        let mut items = vec![];
                                        let from_puse2 = from_pulse.clone()
                                        context.introspect().get_sink_info_list(move |result| match result {
                                            ListResult::Item(item) => {
                                                if let Some(name) = &item.name {
                                                    items.push(name.clone().into_owned())
                                                }
                                            },
                                            ListResult::End => {
                                                let mut lock = from_pulse2.write().unwrap();
                                                lock.push(Message::UpdateSinks(items.clone()))
                                            },
                                            _ => {} //TODO: Match properly
                                        });
                                    },
                                    Message::GetSources => {},
                                    Message::Disconnected => break,
                                }
                            }
                        }
                    });
                    (
                        Some(Event::Connected(Connection(to_pulse))),
                        State::Connected(from_pulse),
                    )
                }
                State::Connected(from_pulse) => {
                    if 0 == (*from_pulse.read().unwrap()).len() {
                        return (None, State::Connected(from_pulse))
                    }
                    return (None, State::Connected(from_pulse))
                }
            }
        },
    )
}

#[derive(Debug)]
enum State {
    Disconnected,
    Connected(
        RwLock<Vec<Message>>,
    ),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected,
    MessageReceived(Message),
}

#[derive(Debug, Clone)]
pub struct Connection(mpsc::Sender<Message>);

impl Connection {
    pub fn send(&mut self, message: Message) {
        let _ = self
            .0
            .send(message)
            .expect("Send message to PulseAudio server");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Connected,
    Disconnected,
    GetSinks,
    GetSources,
    UpdateSinks(Vec<String>),
}
