use crate::future::PAFut;
use iced::futures::FutureExt;

use iced::futures::StreamExt;
use iced_futures::futures;
use iced_native::subscription::{self, Subscription};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, RwLock};
use std::{rc::Rc, thread};
use tokio::runtime::Builder;

extern crate libpulse_binding as pulse;
use futures::channel::mpsc;
use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{Introspector, SinkInfo, SourceInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
        Context, FlagSet,
    },
    error::PAErr,
    mainloop::standard::{IterateResult, Mainloop},
    operation,
    proplist::Proplist,
    volume::ChannelVolumes,
};
pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::unfold(
        std::any::TypeId::of::<Connect>(),
        State::Disconnected,
        |state| async move {
            match state {
                State::Disconnected => {
                    let pulse = PulseHandle::create().unwrap();
                    let (sender, receiver) = mpsc::channel(100);
                    (
                        Some(Event::Connected(Connection(sender))),
                        State::Connected(pulse, receiver),
                    )
                }
                State::Connected(pulse, mut receiver) => {
                    futures::select! {
                        message = receiver.select_next_some() => { match message {
                            Message::GetSinks => (None, State::Connected(pulse, receiver)),
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
    Connected(PulseHandle, mpsc::Receiver<Message>),
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

struct PulseHandle {
    // TODO: One of these should become a mpsc
    to_pulse: Arc<Mutex<Vec<Message>>>,
    from_pulse: Arc<Mutex<Vec<Message>>>,
}

impl PulseHandle {
    // Create pulse server thread, and bidirectional comms
    pub fn create() -> Result<PulseHandle, PAErr> {
        let to_pulse = Arc::new(Mutex::new(vec![]));
        let from_pulse = Arc::new(Mutex::new(vec![]));
        let thread_from_pulse = from_pulse.clone();
        // this thread should complete by pushing a completed message,
        // or fail message. This should never complete/fail without pushing
        // a message. This lets the iced subscription go to sleep while init
        // finishes. TLDR: be very careful with error handling
        thread::spawn(move || {
            let mut pulse_server: Option<PulseServer> = None;
            match PulseServer::connect().and_then(|server| server.init()) {
                Ok(server) => {
                    pulse_server = Some(server);
                    let mut from_channel = thread_from_pulse.lock().unwrap();
                    from_channel.push(Message::Connected);
                }
                Err(_) => {
                    let mut from_channel = thread_from_pulse.lock().unwrap();
                    from_channel.push(Message::Disconnected)
                }
            }
            {
                let mut from_channel = thread_from_pulse.lock().unwrap();
                from_channel.push(Message::Connected)
            }
            loop {}
        });
        Ok(PulseHandle {
            to_pulse,
            from_pulse,
        })
    }
}

struct PulseServer {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>,
    introspector: Introspector,
}

enum PulseServerError {
    IterateErr(IterateResult),
    ContextErr(pulse::context::State),
    OperationErr(pulse::operation::State),
    PAErr(PAErr),
    Connect,
    Misc,
}

impl PulseServer {
    // connect() requires init() to be run after
    pub fn connect() -> Result<PulseServer, PulseServerError> {
        // TODO: fix app name, should be variable
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(
                pulse::proplist::properties::APPLICATION_NAME,
                "com.system76",
            )
            .or(Err(PulseServerError::Connect))?;

        let mainloop = Rc::new(RefCell::new(
            pulse::mainloop::standard::Mainloop::new().ok_or(PulseServerError::Connect)?,
        ));

        let context = Rc::new(RefCell::new(
            Context::new_with_proplist(&*mainloop.borrow(), "MainConn", &proplist)
                .ok_or(PulseServerError::Connect)?,
        ));

        let introspector = context.borrow_mut().introspect();

        context
            .borrow_mut()
            .connect(None, pulse::context::FlagSet::NOFLAGS, None)
            .map_err(|e| PulseServerError::PAErr(e))?;

        Ok(PulseServer {
            mainloop,
            context,
            introspector,
        })
    }

    pub fn init(self) -> Result<Self, PulseServerError> {
        loop {
            match self.mainloop.borrow_mut().iterate(false) {
                IterateResult::Success(_) => {}
                IterateResult::Err(e) => {
                    return Err(PulseServerError::IterateErr(IterateResult::Err(e)))
                }
                IterateResult::Quit(e) => {
                    return Err(PulseServerError::IterateErr(IterateResult::Quit(e)))
                }
            }

            match self.context.borrow().get_state() {
                pulse::context::State::Ready => break,
                pulse::context::State::Failed => {
                    return Err(PulseServerError::ContextErr(pulse::context::State::Failed))
                }
                pulse::context::State::Terminated => {
                    return Err(PulseServerError::ContextErr(
                        pulse::context::State::Terminated,
                    ))
                }
                _ => {}
            }
        }
        Ok(self)
    }

    pub fn get_devices(&self) -> Result<Vec<DeviceInfo>, PulseServerError> {
        let list: Rc<RefCell<Option<Vec<DeviceInfo>>>> = Rc::new(RefCell::new(Some(Vec::new())));
        let list_ref = list.clone();

        let operation = self.introspector.get_sink_info_list(
            move |sink_list: ListResult<&pulse::context::introspect::SinkInfo>| {
                if let ListResult::Item(item) = sink_list {
                    list_ref.borrow_mut().as_mut().unwrap().push(item.into());
                }
            },
        );
        //let mut result = list.borrow_mut();
        //println!("result {:?} ", result.take().unwrap());
        self.wait_for_result(operation)
            .and_then(|_| list.borrow_mut().take().ok_or(PulseServerError::Misc))
            .and_then(|result| Ok(result))
    }

    fn wait_for_result<G: ?Sized>(
        &self,
        operation: pulse::operation::Operation<G>,
    ) -> Result<(), PulseServerError> {
        loop {
            match self.mainloop.borrow_mut().iterate(false) {
                IterateResult::Err(e) => {
                    return Err(PulseServerError::IterateErr(IterateResult::Err(e)))
                }
                IterateResult::Quit(e) => {
                    return Err(PulseServerError::IterateErr(IterateResult::Quit(e)))
                }
                IterateResult::Success(_) => {}
            }
            match operation.get_state() {
                pulse::operation::State::Done => return Ok(()),
                pulse::operation::State::Running => {}
                pulse::operation::State::Cancelled => {
                    return Err(PulseServerError::OperationErr(
                        pulse::operation::State::Cancelled,
                    ))
                }
            }
        }
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
