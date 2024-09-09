// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use std::{cell::RefCell, mem, rc::Rc, thread};

extern crate libpulse_binding as pulse;

use cosmic::{
    iced::{self, subscription},
    iced_futures::futures::{self, SinkExt},
};
use cosmic_time::once_cell::sync::Lazy;

use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{Introspector, SinkInfo, SourceInfo},
        Context,
    },
    error::PAErr,
    mainloop::standard::{IterateResult, Mainloop},
    proplist::Proplist,
    volume::ChannelVolumes,
};

use tokio::sync::{mpsc, Mutex};

pub static FROM_PULSE: Lazy<Mutex<Option<(mpsc::Receiver<Message>, mpsc::Sender<Message>)>>> =
    Lazy::new(|| Mutex::new(None));

pub fn connect() -> iced::Subscription<Event> {
    struct SomeWorker;

    subscription::channel(
        std::any::TypeId::of::<SomeWorker>(),
        50,
        move |mut output| async move {
            let mut state = State::Connecting;

            loop {
                state = start_listening(state, &mut output).await;
            }
        },
    )
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<Event>,
) -> State {
    match state {
        // Waiting for Connection to succeed
        State::Connecting => {
            let mut guard = FROM_PULSE.lock().await;
            let (from_pulse, to_pulse) = {
                if guard.is_none() {
                    let PulseHandle {
                        to_pulse,
                        from_pulse,
                    } = PulseHandle::new();
                    _ = output.send(Event::Init(Connection(to_pulse.clone()))).await;

                    *guard = Some((from_pulse, to_pulse));
                }
                guard.as_mut().unwrap()
            };
            to_pulse
                .send(Message::UpdateConnection)
                .await
                .expect("Failed to request connection update");

            match from_pulse.recv().await {
                Some(Message::Connected) => {
                    _ = output.send(Event::Connected).await;
                    State::Connected
                }
                Some(Message::Disconnected) => {
                    _ = output.send(Event::Disconnected).await;

                    State::Connecting
                }
                Some(m) => {
                    tracing::error!("Unexpected message: {:?}", m);
                    State::Connecting
                }
                None => {
                    panic!("Pulse Sender dropped, something has gone wrong!");
                }
            }
        }
        State::Connected => {
            let mut guard = FROM_PULSE.lock().await;
            let Some((from_pulse, _)) = guard.as_mut() else {
                return State::Connecting;
            };
            // This is where we match messages from the pulse server to pass to the gui
            match from_pulse.recv().await {
                Some(Message::SetSinks(sinks)) => {
                    _ = output
                        .send(Event::MessageReceived(Message::SetSinks(sinks)))
                        .await;

                    State::Connected
                }
                Some(Message::SetSources(sources)) => {
                    _ = output
                        .send(Event::MessageReceived(Message::SetSources(sources)))
                        .await;
                    State::Connected
                }
                Some(Message::SetDefaultSink(sink)) => {
                    _ = output
                        .send(Event::MessageReceived(Message::SetDefaultSink(sink)))
                        .await;
                    State::Connected
                }
                Some(Message::SetDefaultSource(source)) => {
                    _ = output
                        .send(Event::MessageReceived(Message::SetDefaultSource(source)))
                        .await;
                    State::Connected
                }
                Some(Message::Disconnected) => {
                    _ = output.send(Event::Disconnected).await;
                    State::Connecting
                }
                None => {
                    _ = output.send(Event::Disconnected).await;
                    State::Connecting
                }
                _ => State::Connected,
            }
        }
    }
}

// #[derive(Debug)]
enum State {
    Connecting,
    Connected,
}

#[derive(Debug, Clone)]
pub enum Event {
    Init(Connection),
    Connected,
    Disconnected,
    MessageReceived(Message),
}

#[derive(Debug, Clone)]
pub struct Connection(mpsc::Sender<Message>);

impl Connection {
    pub fn send(&mut self, message: Message) {
        if let Err(e) = self.0.try_send(message) {
            match e {
                mpsc::error::TrySendError::Closed(_) => {
                    tracing::error!(
                        "Failed to send message: PulseAudio server communication closed"
                    );
                    panic!();
                }
                mpsc::error::TrySendError::Full(_) => {
                    tracing::warn!("Failed to send message to PulseAudio server: channel is full")
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Connected,
    Disconnected,
    GetSinks,
    GetSources,
    UpdateConnection,
    SetSinks(Vec<DeviceInfo>),
    SetSources(Vec<DeviceInfo>),
    GetDefaultSink,
    GetDefaultSource,
    SetDefaultSink(DeviceInfo),
    SetDefaultSource(DeviceInfo),
    SetSinkVolumeByName(String, ChannelVolumes),
    SetSourceVolumeByName(String, ChannelVolumes),
    SetSinkMuteByName(String, bool),
    SetSourceMuteByName(String, bool),
}

struct PulseHandle {
    to_pulse: tokio::sync::mpsc::Sender<Message>,
    from_pulse: tokio::sync::mpsc::Receiver<Message>,
}

impl PulseHandle {
    // Create pulse server thread, and bidirectional comms
    pub fn new() -> Self {
        let (to_pulse, mut to_pulse_recv) = tokio::sync::mpsc::channel(50);
        let (from_pulse_send, from_pulse) = tokio::sync::mpsc::channel(50);

        // this thread should complete by pushing a completed message,
        // or fail message. This should never complete/fail without pushing
        // a message. This lets the iced subscription go to sleep while init
        // finishes. TLDR: be very careful with error handling
        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            // take `PulseServer` and handle reciver into async context
            // to listen for messages that need to be passed to the pulseserver
            // this lets us put the thread to sleep, but keep hold a single
            // thread, because pulse audio's API is not multithreaded... at all
            rt.block_on(async {
                let mut server: Option<PulseServer> = None;

                let mut msgs = Vec::new();

                loop {
                    if let Some(msg) = to_pulse_recv.recv().await {
                        msgs.push(msg);
                    }

                    // Consume any additional messages in the channel.
                    while let Ok(msg) = to_pulse_recv.try_recv() {
                        // Deduplicate volume change messages.
                        if matches!(
                            msg,
                            Message::SetSinkVolumeByName(..) | Message::SetSourceVolumeByName(..)
                        ) {
                            let last_msg = msgs.last_mut().unwrap(); //
                            if mem::discriminant(last_msg) == mem::discriminant(&msg) {
                                *last_msg = msg;
                                continue;
                            }
                        }

                        msgs.push(msg);
                    }

                    for msg in msgs.drain(..) {
                        match msg {
                            Message::GetDefaultSink => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                match server.get_default_sink() {
                                    Ok(sink) => {
                                        if let Err(err) = from_pulse_send
                                            .send(Message::SetDefaultSink(sink))
                                            .await
                                        {
                                            tracing::error!("ERROR! {}", err);
                                        }
                                    }
                                    Err(_) => Self::send_disconnected(&from_pulse_send).await,
                                }
                            }
                            Message::GetDefaultSource => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                match server.get_default_source() {
                                    Ok(source) => {
                                        if let Err(err) = from_pulse_send
                                            .send(Message::SetDefaultSource(source))
                                            .await
                                        {
                                            tracing::error!("ERROR! {}", err);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("ERROR! {:?}", e);
                                        Self::send_disconnected(&from_pulse_send).await;
                                    }
                                }
                            }
                            Message::GetSinks => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                match server.get_sinks() {
                                    Ok(sinks) => {
                                        if let Err(err) =
                                            from_pulse_send.send(Message::SetSinks(sinks)).await
                                        {
                                            tracing::error!("ERROR! {}", err);
                                        }
                                    }
                                    Err(_) => Self::send_disconnected(&from_pulse_send).await,
                                }
                            }
                            Message::GetSources => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                match server.get_sources() {
                                    Ok(sinks) => {
                                        if let Err(err) =
                                            from_pulse_send.send(Message::SetSources(sinks)).await
                                        {
                                            tracing::error!("ERROR! {}", err);
                                        }
                                    }
                                    Err(_) => Self::send_disconnected(&from_pulse_send).await,
                                }
                            }
                            Message::SetSinkVolumeByName(name, channel_volumes) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                server.set_sink_volume_by_name(&name, &channel_volumes)
                            }
                            Message::SetSourceVolumeByName(name, channel_volumes) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                server.set_source_volume_by_name(&name, &channel_volumes)
                            }
                            Message::SetSinkMuteByName(name, mute) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };

                                let op =
                                    server.introspector.set_sink_mute_by_name(&name, mute, None);
                                server.wait_for_result(op).ok();
                            }
                            Message::SetSourceMuteByName(name, mute) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };

                                let op = server
                                    .introspector
                                    .set_source_mute_by_name(&name, mute, None);
                                server.wait_for_result(op).ok();
                            }
                            Message::UpdateConnection => {
                                tracing::info!(
                                    "Updating Connection, server exists: {:?}",
                                    server.is_some()
                                );
                                if let Some(mut cur_server) = server.take() {
                                    if cur_server.get_server_info().is_err() {
                                        tracing::warn!("got error, server must be disconnected...");
                                        Self::send_disconnected(&from_pulse_send).await;
                                    } else {
                                        tracing::info!("got server info, still connected...");
                                        server = Some(cur_server);
                                        Self::send_connected(&from_pulse_send).await;
                                    }
                                } else {
                                    match PulseServer::connect().and_then(|server| server.init()) {
                                        Ok(new_server) => {
                                            tracing::info!("Connected to server");
                                            Self::send_connected(&from_pulse_send).await;
                                            server = Some(new_server);
                                        }
                                        Err(err) => {
                                            tracing::error!(
                                                "Failed to connect to server: {:?}",
                                                err
                                            );
                                            Self::send_disconnected(&from_pulse_send).await;
                                        }
                                    }
                                }
                            }
                            Message::SetDefaultSink(device) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                let default_sink = match server.get_default_sink() {
                                    Ok(sink) => sink,
                                    Err(_) => continue,
                                };
                                let to_move = server.get_sink_inputs(default_sink.index);
                                if let Some(name) = device.name.as_ref() {
                                    if server.set_default_sink(name, to_move) {
                                        if let Err(err) = from_pulse_send
                                            .send(Message::SetDefaultSink(device))
                                            .await
                                        {
                                            tracing::error!("ERROR! {:?}", err);
                                        };
                                    }
                                }
                            }
                            Message::SetDefaultSource(device) => {
                                let server = match server.as_mut() {
                                    Some(s) => s,
                                    None => continue,
                                };
                                let default_source = match server.get_default_source() {
                                    Ok(source) => source,
                                    Err(_) => continue,
                                };
                                let to_move = server.get_source_outputs(default_source.index);
                                if let Some(name) = device.name.as_ref() {
                                    if server.set_default_source(name, to_move) {
                                        if let Err(err) = from_pulse_send
                                            .send(Message::SetDefaultSource(device))
                                            .await
                                        {
                                            tracing::error!("ERROR! {:?}", err);
                                        };
                                    }
                                }
                            }
                            _ => {
                                tracing::warn!("message doesn't match")
                            }
                        }
                    }
                }
            });
        });
        Self {
            to_pulse,
            from_pulse,
        }
    }

    async fn send_disconnected(sender: &tokio::sync::mpsc::Sender<Message>) {
        sender.send(Message::Disconnected).await.unwrap()
    }

    #[allow(dead_code)]
    async fn send_connected(sender: &tokio::sync::mpsc::Sender<Message>) {
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
    pub fn connect() -> Result<Self, PulseServerError<'static>> {
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
            .map_err(PulseServerError::PAErr)?;

        Ok(Self {
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
        self.wait_for_result(operation).and_then(|_| {
            list.borrow_mut().take().ok_or(PulseServerError::Misc(
                "get_sinks(): failed to wait for operation",
            ))
        })
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
        self.wait_for_result(operation).and_then(|_| {
            list.borrow_mut().take().ok_or(PulseServerError::Misc(
                "get_sources(): Failed to wait for operation",
            ))
        })
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

    fn set_default_sink(&mut self, sink: &str, to_move: Vec<u32>) -> bool {
        let set_default_success = Rc::new(RefCell::new(false));
        let set_default_success_ref = set_default_success.clone();
        let op = self
            .context
            .borrow_mut()
            .set_default_sink(sink, move |ret| {
                *set_default_success.borrow_mut() = ret;
            });
        self.wait_for_result(op).ok();
        if !set_default_success_ref.replace(true) {
            return false;
        }

        for index in to_move {
            let move_success = Rc::new(RefCell::new(false));
            let op = self.introspector.move_sink_input_by_name(
                index,
                sink,
                Some(Box::new(move |ret| {
                    *move_success.borrow_mut() = ret;
                })),
            );

            self.wait_for_result(op).ok();
        }
        // TODO handle errors
        true
    }

    fn set_default_source(&mut self, sink: &str, to_move: Vec<u32>) -> bool {
        let set_default_success = Rc::new(RefCell::new(false));
        let set_default_success_ref = set_default_success.clone();
        let op = self
            .context
            .borrow_mut()
            .set_default_source(sink, move |ret| {
                *set_default_success.borrow_mut() = ret;
            });
        self.wait_for_result(op).ok();

        if !set_default_success_ref.replace(true) {
            return false;
        }

        for index in to_move {
            let move_success = Rc::new(RefCell::new(false));
            let op = self.introspector.move_source_output_by_name(
                index,
                sink,
                Some(Box::new(move |ret| {
                    *move_success.borrow_mut() = ret;
                })),
            );

            self.wait_for_result(op).ok();
        }

        true
    }

    fn get_default_sink(&mut self) -> Result<DeviceInfo, PulseServerError> {
        let server_info = self.get_server_info();
        match server_info {
            Ok(info) => {
                let name = &info.default_sink_name.unwrap_or_default();
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
                result.take().unwrap().ok_or({
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
                let name = &info.default_source_name.unwrap_or_default();
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
                result.take().unwrap().ok_or({
                    PulseServerError::Misc("get_default_source(): Error getting requested device")
                })
            }
            Err(_) => Err(PulseServerError::Misc("get_default_source() failed")),
        }
    }

    fn set_sink_volume_by_name(&mut self, name: &str, volume: &ChannelVolumes) {
        let op = self
            .introspector
            .set_sink_mute_by_name(name, volume.is_muted(), None);
        self.wait_for_result(op).ok();

        let op = self
            .introspector
            .set_sink_volume_by_name(name, volume, None);
        self.wait_for_result(op).ok();
    }

    fn set_source_volume_by_name(&mut self, name: &str, volume: &ChannelVolumes) {
        let op = self
            .introspector
            .set_source_mute_by_name(name, volume.is_muted(), None);
        let _ = self.wait_for_result(op);

        let op = self
            .introspector
            .set_source_volume_by_name(name, volume, None);
        let _ = self.wait_for_result(op);
    }

    fn get_source_outputs(&mut self, source: u32) -> Vec<u32> {
        let result = Rc::new(RefCell::new(Vec::new()));
        let result_ref = Rc::new(RefCell::new(Vec::new()));
        let op = self.introspector.get_source_output_info_list(move |list| {
            if let ListResult::Item(item) = list {
                if source == item.source {
                    result.borrow_mut().push(item.index);
                }
            }
        });
        let _ = self.wait_for_result(op);
        result_ref.replace(Vec::new())
    }

    fn get_sink_inputs(&mut self, sink: u32) -> Vec<u32> {
        let result = Rc::new(RefCell::new(Vec::new()));
        let result_ref = Rc::new(RefCell::new(Vec::new()));
        let op = self.introspector.get_sink_input_info_list(move |list| {
            if let ListResult::Item(item) = list {
                if sink == item.sink {
                    result.borrow_mut().push(item.index);
                }
            }
        });
        let _ = self.wait_for_result(op);
        result_ref.replace(Vec::new())
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
        Self {
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
