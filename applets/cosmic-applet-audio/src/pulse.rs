use crate::future::PAFut;
use iced::futures::FutureExt;

use iced_futures::futures;
use iced_native::subscription::{self, Subscription};
use std::sync::{Arc, RwLock};
use iced::futures::StreamExt;
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
use futures::channel::mpsc;
pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::unfold(
        std::any::TypeId::of::<Connect>(),
        State::Disconnected,
        |state| async move {
            match state {
                State::Disconnected => {
                    let pulse = PulseServer::new("com.system76.cosmic.applets.audio").unwrap();
                    let (sender, receiver) = mpsc::channel(100);
                    (
                        Some(Event::Connected(Connection(sender))),
                        State::Connected(pulse, receiver),
                    )
                }
                State::Connected(pulse, mut receiver) => {
                    futures::select! {
                        message = receiver.select_next_some() => { match message {
                            Message::GetSinks => {
                                if let Ok(sinks) = pulse.get_sinks().await {
                                    (
                                        Some(Event::MessageReceived(Message::SetSinks(sinks))),
                                        State::Connected(pulse, receiver),
                                    )
                                } else {
                                    (None, State::Connected(pulse, receiver))
                                }
                            },
                            _ => (None, State::Connected(pulse, receiver)),
                        }}
                    }
                }
            }
        },
    )
}

// #[derive(Debug)]
enum State {
    Disconnected,
    Connected(
        PulseServer,
        mpsc::Receiver<Message>,
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
            .try_send(message)
            .expect("Send message to PulseAudio server");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Connected,
    Disconnected,
    GetSinks,
    GetSources,
    SetSinks(Vec<DeviceInfo>),
}

struct PulseServer {
    context: Context,
}

impl PulseServer {
    pub fn new(context: &str) -> Result<PulseServer, PAErr> {
        let mut mainloop = pulse::mainloop::threaded::Mainloop::new().unwrap();
        mainloop.start()?;
        Ok(PulseServer {
            context: Context::new(&mainloop, context).unwrap(),
        })
    }

    fn introspect(&self) -> Introspector {
        self.context.introspect()
    }

    pub async fn get_sinks(&self) -> Result<Vec<DeviceInfo>, ()> {
        let mut items = Some(Vec::new());
        self.introspect()
            .get_sink_info_list(move |result| match result {
                ListResult::Item(item) => items.as_mut().unwrap().push(DeviceInfo::from(item)),
                ListResult::End => waker.wake(Ok(items.take().unwrap())),
                ListResult::Error => waker.wake(Err(())),
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub volume: ChannelVolumes,
    pub mute: bool,
    pub index: u32,
}

impl<'a> From<&SinkInfo<'a>> for DeviceInfo {
    fn from(info: &SinkInfo<'a>) -> Self {
        Self {
            name: info.name.clone().map(|x| x.into_owned()),
            description: info.description.clone().map(|x| x.into_owned()),
            volume: info.volume,
            mute: info.mute,
            index: info.index,
        }
    }
}

impl Eq for DeviceInfo {}
