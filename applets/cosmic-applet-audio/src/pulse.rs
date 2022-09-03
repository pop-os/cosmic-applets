use iced_native::subscription::{self, Subscription};
use std::cell::RefCell;
use std::{rc::Rc, thread};

extern crate libpulse_binding as pulse;
//use futures::channel::mpsc;
use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{Introspector, SinkInfo, SourceInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
        Context,
    },
    volume::ChannelVolumes,
    mainloop::standard::{IterateResult, Mainloop},
    proplist::Proplist,
    error::PAErr,
};
pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::unfold(
        std::any::TypeId::of::<Connect>(),
        State::Disconnected,
        |state| async move {
            match state {
                // if app just started, or we are re-trying match here. Returns coenncting
                // message. We should store this in our app's state, but it isn't safe to
                // send messages until we get a conencted message. Which will be received
                // by the `State::Connecting` message below
                State::Disconnected => match PulseHandle::create() {
                    Ok(pulse_handle) => (None, State::Connecting(pulse_handle)),
                    Err(_) => (Some(Event::Disconnected), State::Disconnected),
                },
                // Just a buffer to make sure the GUI doesn't send messages until pulse is ready
                // The GUI doesn't have to monitor this state, as it is never sent to the GUI
                State::Connecting(mut pulse_handle) => {
                        match pulse_handle.from_pulse.recv().await {
                            Some(Message::Connected) => {(
                                    Some(Event::Connected(Connection(pulse_handle.to_pulse))),
                                    State::Connected(pulse_handle.from_pulse),
                            )}
                            Some(Message::Disconnected) => (Some(Event::Disconnected), State::Disconnected),
                            _ => panic!("Pulse subscription logic is faulty as the PulseServer shouldn't send unique messages until connection is successful")
                        }
                },
                State::Connected(mut from_pulse) => {
                    // This is where we match messages from the pulse server to pass to the gui
                        match from_pulse.recv().await {
                            Some(Message::SetSinks(sinks)) => (Some(Event::MessageReceived(Message::SetSinks(sinks))), State::Connected(from_pulse)),
                            Some(Message::SetSources(sources)) => (Some(Event::MessageReceived(Message::SetSources(sources))), State::Connected(from_pulse)),
                            Some(Message::SetDefaultSink(sink)) => (Some(Event::MessageReceived(Message::SetDefaultSink(sink))), State::Connected(from_pulse)),
                            Some(Message::SetDefaultSource(source)) => (Some(Event::MessageReceived(Message::SetDefaultSource(source))), State::Connected(from_pulse)),
                            Some(Message::Disconnected) => (Some(Event::Disconnected), State::Disconnected),
                            None => (Some(Event::Disconnected), State::Disconnected),
                            _ => (None, State::Connected(from_pulse)),
                        }
                }
            }
        },
    )
}

// #[derive(Debug)]
enum State {
    Disconnected,
    Connecting(PulseHandle),
    Connected(tokio::sync::mpsc::Receiver<Message>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected,
    MessageReceived(Message),
}

#[derive(Debug, Clone)]
pub struct Connection(tokio::sync::mpsc::Sender<Message>);

impl Connection {
    pub fn send(&mut self, message: Message) {
        let _ = self
            .0
            .try_send(message)
            .expect("Send message to PulseAudio server");
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Connected,
    Disconnected,
    GetSinks,
    GetSources,
    SetSinks(Vec<DeviceInfo>),
    SetSources(Vec<DeviceInfo>),
    GetDefaultSink,
    GetDefaultSource,
    SetDefaultSink(DeviceInfo),
    SetDefaultSource(DeviceInfo),
    SetDeviceVolumeByName(String, ChannelVolumes)
}

struct PulseHandle {
    to_pulse: tokio::sync::mpsc::Sender<Message>,
    from_pulse: tokio::sync::mpsc::Receiver<Message>,
}

impl PulseHandle {
    // Create pulse server thread, and bidirectional comms
    pub fn create() -> Result<PulseHandle, PAErr> {
        let (to_pulse, mut to_pulse_recv) = tokio::sync::mpsc::channel(10);
        let (mut from_pulse_send, from_pulse) = tokio::sync::mpsc::channel(10);
        //let from_pulse = Arc::new(Mutex::new(vec![]));
        //let mut from_pulse2 = from_pulse.clone();
        // this thread should complete by pushing a completed message,
        // or fail message. This should never complete/fail without pushing
        // a message. This lets the iced subscription go to sleep while init
        // finishes. TLDR: be very careful with error handling
        thread::spawn(move || {
            if let Ok(mut server) = PulseServer::connect().and_then(|server| server.init()) {
                PulseHandle::blocking_send_connected(&mut from_pulse_send);

                // take `PulseServer` and handle reciver into async context
                // to listen for messages that need to be passed to the pulseserver
                // this lets us put the thread to sleep, but keep hold a single
                // thread, because pulse audio's API is not multithreaded... at all
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async {
                    loop {
                        // This is where the we match messages from the GUI to pass to the pulse server
                        if let Some(msg) = to_pulse_recv.recv().await {
                            match msg {
                                Message::GetDefaultSink => match server.get_default_sink() {
                                    Ok(sink) => from_pulse_send
                                        .send(Message::SetDefaultSink(sink))
                                        .await
                                        .unwrap(),
                                    Err(_) => {
                                        PulseHandle::send_disconnected(&mut from_pulse_send).await
                                    }
                                },
                                Message::GetDefaultSource => match server.get_default_source() {
                                    Ok(source) => from_pulse_send
                                        .send(Message::SetDefaultSource(source))
                                        .await
                                        .unwrap(),
                                    Err(e) => {
                                        println!("ERROR! {:?}", e);
                                        PulseHandle::send_disconnected(&mut from_pulse_send).await;
                                    }
                                },
                                Message::GetSinks => match server.get_sinks() {
                                    Ok(sinks) => from_pulse_send
                                        .send(Message::SetSinks(sinks))
                                        .await
                                        .unwrap(),
                                    Err(_) => {
                                        PulseHandle::send_disconnected(&mut from_pulse_send).await
                                    }
                                },
                                Message::GetSources => match server.get_sources() {
                                    Ok(sinks) => from_pulse_send
                                        .send(Message::SetSources(sinks))
                                        .await
                                        .unwrap(),
                                    Err(_) => {
                                        PulseHandle::send_disconnected(&mut from_pulse_send).await
                                    }
                                },
                                Message::SetDeviceVolumeByName(name, channel_volumes) => {
                                    server.set_device_volume_by_name(&name, &channel_volumes)
                                },
                                _ => {
                                    println!("message doesn't match")
                                }
                            }
                        }
                    }
                });
            }
            // Always report that server is disconnected
            PulseHandle::blocking_send_disconnected(&mut from_pulse_send);
        });
        Ok(PulseHandle {
            to_pulse,
            from_pulse,
        })
    }

    fn blocking_send_disconnected(sender: &mut tokio::sync::mpsc::Sender<Message>) {
        sender.blocking_send(Message::Disconnected);
    }

    fn blocking_send_connected(sender: &mut tokio::sync::mpsc::Sender<Message>) {
        sender.blocking_send(Message::Connected).unwrap()
    }

    async fn send_disconnected(sender: &mut tokio::sync::mpsc::Sender<Message>) {
        sender.send(Message::Disconnected).await.unwrap()
    }

    async fn send_connected(sender: &mut tokio::sync::mpsc::Sender<Message>) {
        sender.send(Message::Connected).await.unwrap()
    }
}

struct PulseServer {
    mainloop: Rc<RefCell<Mainloop>>,
    context: Rc<RefCell<Context>>,
    introspector: Introspector,
}

#[derive(Clone, Debug)]
enum PulseServerError<'a> {
    IterateErr(IterateResult),
    ContextErr(pulse::context::State),
    OperationErr(pulse::operation::State),
    PAErr(PAErr),
    Connect,
    Misc(&'a str),
}

// `PulseServer` code is heavily inspired by Dave Patrick Caberto's pulsectl-rs (SeaDve)
// https://crates.io/crates/pulsectl-rs
impl PulseServer {
    // connect() requires init() to be run after
    pub fn connect() -> Result<PulseServer, PulseServerError<'static>> {
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

    // Wait for pulse audio connection to complete
    pub fn init(self) -> Result<Self, PulseServerError<'static>> {
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

    // Get a list of output devices
    pub fn get_sinks(&self) -> Result<Vec<DeviceInfo>, PulseServerError> {
        let list: Rc<RefCell<Option<Vec<DeviceInfo>>>> = Rc::new(RefCell::new(Some(Vec::new())));
        let list_ref = list.clone();

        let operation = self.introspector.get_sink_info_list(
            move |sink_list: ListResult<&pulse::context::introspect::SinkInfo>| {
                if let ListResult::Item(item) = sink_list {
                    list_ref.borrow_mut().as_mut().unwrap().push(item.into());
                }
            },
        );
        self.wait_for_result(operation)
            .and_then(|_| {
                list.borrow_mut().take().ok_or(PulseServerError::Misc(
                    "get_sinks(): failed to wait for operation",
                ))
            })
            .and_then(|result| Ok(result))
    }

    // Get a list of input devices
    pub fn get_sources(&self) -> Result<Vec<DeviceInfo>, PulseServerError> {
        let list: Rc<RefCell<Option<Vec<DeviceInfo>>>> = Rc::new(RefCell::new(Some(Vec::new())));
        let list_ref = list.clone();

        let operation = self.introspector.get_source_info_list(
            move |sink_list: ListResult<&pulse::context::introspect::SourceInfo>| {
                if let ListResult::Item(item) = sink_list {
                    list_ref.borrow_mut().as_mut().unwrap().push(item.into());
                }
            },
        );
        self.wait_for_result(operation)
            .and_then(|_| {
                list.borrow_mut().take().ok_or(PulseServerError::Misc(
                    "get_sources(): Failed to wait for operation",
                ))
            })
            .and_then(|result| Ok(result))
    }

    pub fn get_server_info(&mut self) -> Result<ServerInfo, PulseServerError> {
        let info = Rc::new(RefCell::new(Some(None)));
        let info_ref = info.clone();

        let op = self.introspector.get_server_info(move |res| {
            info_ref.borrow_mut().as_mut().unwrap().replace(res.into());
        });
        self.wait_for_result(op)?;
        info.take()
            .flatten()
            .ok_or(PulseServerError::Misc("get_server_info(): failed"))
    }

    fn get_default_sink(&mut self) -> Result<DeviceInfo, PulseServerError> {
        let server_info = self.get_server_info();
        match server_info {
            Ok(info) => {
                let name = &info.default_sink_name.unwrap_or(String::new());
                let device = Rc::new(RefCell::new(Some(None)));
                let dev_ref = device.clone();
                let op = self.introspector.get_sink_info_by_name(
                    name,
                    move |sink_list: ListResult<&SinkInfo>| {
                        if let ListResult::Item(item) = sink_list {
                            dev_ref.borrow_mut().as_mut().unwrap().replace(item.into());
                        }
                    },
                );
                self.wait_for_result(op)?;
                let mut result = device.borrow_mut();
                result.take().unwrap().ok_or_else(|| {
                    PulseServerError::Misc("get_default_sink(): Error getting requested device")
                })
            }
            Err(_) => Err(PulseServerError::Misc("get_default_sink() failed")),
        }
    }

    fn get_default_source(&mut self) -> Result<DeviceInfo, PulseServerError> {
        let server_info = self.get_server_info();
        match server_info {
            Ok(info) => {
                let name = &info.default_source_name.unwrap_or(String::new());
                let device = Rc::new(RefCell::new(Some(None)));
                let dev_ref = device.clone();
                let op = self.introspector.get_source_info_by_name(
                    name,
                    move |sink_list: ListResult<&SourceInfo>| {
                        if let ListResult::Item(item) = sink_list {
                            dev_ref.borrow_mut().as_mut().unwrap().replace(item.into());
                        }
                    },
                );
                self.wait_for_result(op)?;
                let mut result = device.borrow_mut();
                result.take().unwrap().ok_or_else(|| {
                    PulseServerError::Misc("get_default_source(): Error getting requested device")
                })
            }
            Err(_) => Err(PulseServerError::Misc("get_default_source() failed")),
        }
    }

    fn set_device_volume_by_name(&mut self, name: &str, volume: &ChannelVolumes) {
        let op = self
            .introspector
            .set_sink_volume_by_name(name, volume, None);
        self.wait_for_result(op).ok();
    }

    // after building an operation such as get_devices() we need to keep polling
    // the pulse audio server to "wait" for the operation to complete
    fn wait_for_result<G: ?Sized>(
        &self,
        operation: pulse::operation::Operation<G>,
    ) -> Result<(), PulseServerError> {
        // TODO: make this loop async. It is already in an async context, so
        // we could make this thread sleep while waiting for the pulse server's
        // response.
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

impl<'a> From<&SourceInfo<'a>> for DeviceInfo {
    fn from(info: &SourceInfo<'a>) -> Self {
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

#[derive(Debug)]
pub struct ServerInfo {
    /// User name of the daemon process.
    pub user_name: Option<String>,
    /// Host name the daemon is running on.
    pub host_name: Option<String>,
    /// Version string of the daemon.
    pub server_version: Option<String>,
    /// Server package name (usually “pulseaudio”).
    pub server_name: Option<String>,
    // Default sample specification.
    //pub sample_spec: sample::Spec,
    /// Name of default sink.
    pub default_sink_name: Option<String>,
    /// Name of default source.
    pub default_source_name: Option<String>,
    /// A random cookie for identifying this instance of PulseAudio.
    pub cookie: u32,
    // Default channel map.
    //pub channel_map: channelmap::Map,
}

impl<'a> From<&'a pulse::context::introspect::ServerInfo<'a>> for ServerInfo {
    fn from(info: &'a pulse::context::introspect::ServerInfo<'a>) -> Self {
        ServerInfo {
            user_name: info.user_name.as_ref().map(|cow| cow.to_string()),
            host_name: info.host_name.as_ref().map(|cow| cow.to_string()),
            server_version: info.server_version.as_ref().map(|cow| cow.to_string()),
            server_name: info.server_name.as_ref().map(|cow| cow.to_string()),
            //sample_spec: info.sample_spec,
            default_sink_name: info.default_sink_name.as_ref().map(|cow| cow.to_string()),
            default_source_name: info.default_source_name.as_ref().map(|cow| cow.to_string()),
            cookie: info.cookie,
            //channel_map: info.channel_map,
        }
    }
}
