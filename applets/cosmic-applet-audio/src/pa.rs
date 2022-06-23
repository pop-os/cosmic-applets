use futures::{channel::oneshot, future::poll_fn, task::Poll};
use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{Introspector, SinkInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
        Context, FlagSet, State,
    },
    error::PAErr,
    volume::ChannelVolumes,
};
use libpulse_glib_binding::Mainloop;
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

pub struct DeviceInfo {
    pub name: Option<String>,
    pub description: Option<String>,
    pub volume: ChannelVolumes,
}

pub struct ServerInfo {
    pub default_sink_name: Option<String>,
    pub default_source_name: Option<String>,
}

struct PAInner {
    main_loop: Mainloop,
    pub context: RefCell<Context>,
}

#[derive(Clone)]
pub struct PA(Rc<PAInner>);

impl PA {
    pub fn new() -> Option<Self> {
        let main_loop = Mainloop::new(None)?;
        let context = Context::new(&main_loop, "com.system76.cosmic.applets.audio")?;
        Some(Self(Rc::new(PAInner {
            main_loop,
            context: RefCell::new(context),
        })))
    }

    pub fn set_state_callback<F: Fn(&Self, State) + 'static>(&self, cb: F) {
        let pa = self.clone(); // TODO: weak ref?
        self.0
            .context
            .borrow_mut()
            .set_state_callback(Some(Box::new(move || {
                let state = pa.0.context.borrow().get_state();
                cb(&pa, state);
            })));
    }

    // TODO: builder pattern?
    pub fn set_subscribe_callback<F: FnMut(Option<Facility>, Option<Operation>, u32) + 'static>(
        &self,
        cb: F,
    ) {
        self.0
            .context
            .borrow_mut()
            .set_subscribe_callback(Some(Box::new(cb)));
    }

    pub fn subscribe(&self, mask: InterestMaskSet) {
        // XXX cb; operation; async
        self.0.context.borrow_mut().subscribe(mask, |_| {});
    }

    pub fn connect(&self) -> Result<(), PAErr> {
        self.0
            .context
            .borrow_mut()
            .connect(None, FlagSet::empty(), None)
    }

    fn introspect(&self) -> Introspector {
        self.0.context.borrow().introspect()
    }

    pub async fn get_server_info(&self) -> ServerInfo {
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        self.introspect().get_server_info(move |info| {
            sender.take().unwrap().send(ServerInfo {
                default_sink_name: info.default_sink_name.clone().map(|x| x.into_owned()),
                default_source_name: info.default_source_name.clone().map(|x| x.into_owned()),
            });
        });
        receiver.await.unwrap()
    }

    pub async fn get_sink_info_list(&self) -> Result<Vec<DeviceInfo>, ()> {
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        let mut items = Some(Vec::new());
        self.introspect()
            .get_sink_info_list(move |result| match result {
                ListResult::Item(item) => items.as_mut().unwrap().push(DeviceInfo {
                    name: item.name.clone().map(|x| x.into_owned()),
                    description: item.description.clone().map(|x| x.into_owned()),
                    volume: item.volume,
                }),
                ListResult::End => {
                    sender.take().unwrap().send(Ok(items.take().unwrap()));
                }
                ListResult::Error => {
                    sender.take().unwrap().send(Err(()));
                }
            });
        receiver.await.unwrap()
    }

    pub async fn get_default_sink(&self) -> Result<DeviceInfo, ()> {
        let name = match self.get_server_info().await.default_sink_name {
            Some(name) => name,
            None => {
                return Err(());
            }
        };
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        let mut sink = None;
        self.introspect()
            .get_sink_info_by_name(&name, move |result| match result {
                ListResult::Item(item) => {
                    sink = Some(DeviceInfo {
                        name: item.name.clone().map(|x| x.into_owned()),
                        description: item.description.clone().map(|x| x.into_owned()),
                        volume: item.volume,
                    });
                }
                ListResult::End => {
                    sender.take().unwrap().send(sink.take().ok_or(()));
                }
                ListResult::Error => {
                    sender.take().unwrap().send(Err(()));
                }
            });
        receiver.await.unwrap()
    }

    /*
    // XXX async wait and handle error
    pub fn set_default_sink(&mut self, name: &str) {
        self.0.context.set_default_sink(name, |_| {});
    }
    */

    pub async fn get_source_info_list(&self) -> Result<Vec<DeviceInfo>, ()> {
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        let mut items = Some(Vec::new());
        self.introspect()
            .get_source_info_list(move |result| match result {
                ListResult::Item(item) => items.as_mut().unwrap().push(DeviceInfo {
                    name: item.name.clone().map(|x| x.into_owned()),
                    description: item.description.clone().map(|x| x.into_owned()),
                    volume: item.volume,
                }),
                ListResult::End => {
                    sender.take().unwrap().send(Ok(items.take().unwrap()));
                }
                ListResult::Error => {
                    sender.take().unwrap().send(Err(()));
                }
            });
        receiver.await.unwrap()
    }

    pub async fn get_default_source(&self) -> Result<DeviceInfo, ()> {
        let name = match self.get_server_info().await.default_source_name {
            Some(name) => name,
            None => {
                return Err(());
            }
        };
        let (sender, receiver) = oneshot::channel();
        let mut sender = Some(sender);
        let mut source = None;
        self.introspect()
            .get_source_info_by_name(&name, move |result| match result {
                ListResult::Item(item) => {
                    source = Some(DeviceInfo {
                        name: item.name.clone().map(|x| x.into_owned()),
                        description: item.description.clone().map(|x| x.into_owned()),
                        volume: item.volume,
                    });
                }
                ListResult::End => {
                    sender.take().unwrap().send(source.take().ok_or(()));
                }
                ListResult::Error => {
                    sender.take().unwrap().send(Err(()));
                }
            });
        receiver.await.unwrap()
    }
}
