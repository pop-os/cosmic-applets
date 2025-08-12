// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::wayland_subscription::{
    ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};
use std::{
    os::{
        fd::{AsFd, FromRawFd, RawFd},
        unix::net::UnixStream,
    },
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

use cctk::{
    sctk::{
        self,
        reexports::{calloop, calloop_wayland_source::WaylandSource},
        seat::{SeatHandler, SeatState},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{self, WEnum, protocol::wl_seat::WlSeat},
};
use cosmic::{
    cctk::{
        self,
        cosmic_protocols::{
            self, toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        },
        screencopy::{
            CaptureFrame, CaptureOptions, CaptureSession, CaptureSource, Capturer, FailureReason,
            Formats, Frame, ScreencopyFrameData, ScreencopyFrameDataExt, ScreencopyHandler,
            ScreencopySessionData, ScreencopySessionDataExt, ScreencopyState,
        },
        sctk::shm::{Shm, ShmHandler},
        wayland_client::{
            Dispatch,
            protocol::{
                wl_buffer,
                wl_shm::{self, WlShm},
                wl_shm_pool,
            },
        },
        wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    },
    iced_futures::futures,
};
use cosmic_protocols::{
    toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
};
use futures::{SinkExt, channel::mpsc};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use wayland_client::{Connection, QueueHandle, globals::registry_queue_init};

#[derive(Default)]
struct SessionInner {
    formats: Option<Formats>,
    res: Option<Result<(), WEnum<FailureReason>>>,
}

// TODO: dmabuf? need to handle modifier negotation
#[derive(Default)]
struct Session {
    condvar: Condvar,
    inner: Mutex<SessionInner>,
}

#[derive(Default)]
struct SessionData {
    session: Arc<Session>,
    session_data: ScreencopySessionData,
}

struct FrameData {
    frame_data: ScreencopyFrameData,
    session: CaptureSession,
}

impl Session {
    pub fn for_session(session: &CaptureSession) -> Option<&Self> {
        Some(&session.data::<SessionData>()?.session)
    }

    fn update<F: FnOnce(&mut SessionInner)>(&self, f: F) {
        f(&mut self.inner.lock().unwrap());
        self.condvar.notify_all();
    }

    fn wait_while<F: FnMut(&SessionInner) -> bool>(&self, mut f: F) -> MutexGuard<SessionInner> {
        self.condvar
            .wait_while(self.inner.lock().unwrap(), |data| f(data))
            .unwrap()
    }
}

impl ScreencopySessionDataExt for SessionData {
    fn screencopy_session_data(&self) -> &ScreencopySessionData {
        &self.session_data
    }
}

impl ScreencopyFrameDataExt for FrameData {
    fn screencopy_frame_data(&self) -> &ScreencopyFrameData {
        &self.frame_data
    }
}

struct AppData {
    exit: bool,
    tx: mpsc::Sender<WaylandUpdate>,
    queue_handle: QueueHandle<Self>,
    conn: Connection,
    screencopy_state: ScreencopyState,
    shm_state: Shm,
    registry_state: RegistryState,
    toplevel_info_state: ToplevelInfoState,
    toplevel_manager_state: ToplevelManagerState,
    seat_state: SeatState,
}

struct CaptureData {
    qh: QueueHandle<AppData>,
    conn: Connection,
    wl_shm: WlShm,
    capturer: Capturer,
}

impl CaptureData {
    pub fn capture_source_shm_fd<Fd: AsFd>(
        &self,
        overlay_cursor: bool,
        source: ExtForeignToplevelHandleV1,
        fd: Fd,
        len: Option<u32>,
    ) -> Option<ShmImage<Fd>> {
        // XXX error type?
        // TODO: way to get cursor metadata?

        #[allow(unused_variables)] // TODO
        let overlay_cursor = if overlay_cursor { 1 } else { 0 };

        let session = Arc::new(Session::default());
        let capture_session = self
            .capturer
            .create_session(
                &CaptureSource::Toplevel(source),
                CaptureOptions::empty(),
                &self.qh,
                SessionData {
                    session: session.clone(),
                    session_data: Default::default(),
                },
            )
            .unwrap();
        self.conn.flush().unwrap();

        let formats = session
            .wait_while(|data| data.formats.is_none())
            .formats
            .take()
            .unwrap();
        let (width, height) = formats.buffer_size;

        if width == 0 || height == 0 {
            return None;
        }

        // XXX
        if !formats
            .shm_formats
            .contains(&wl_shm::Format::Abgr8888.into())
        {
            tracing::error!("No suitable buffer format found");
            tracing::warn!("Available formats: {:#?}", formats);
            return None;
        };

        let buf_len = width * height * 4;
        if let Some(len) = len {
            if len != buf_len {
                return None;
            }
        } else if let Err(_err) = rustix::fs::ftruncate(&fd, buf_len as _) {
        };
        let pool = self
            .wl_shm
            .create_pool(fd.as_fd(), buf_len as i32, &self.qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Abgr8888,
            &self.qh,
            (),
        );

        capture_session.capture(
            &buffer,
            &[],
            &self.qh,
            FrameData {
                frame_data: Default::default(),
                session: capture_session.clone(),
            },
        );
        self.conn.flush().unwrap();

        // TODO: wait for server to release buffer?
        let res = session
            .wait_while(|data| data.res.is_none())
            .res
            .take()
            .unwrap();
        pool.destroy();
        buffer.destroy();

        //std::thread::sleep(std::time::Duration::from_millis(16));

        if res.is_ok() {
            Some(ShmImage { fd, width, height })
        } else {
            None
        }
    }
}

pub struct ShmImage<T: AsFd> {
    fd: T,
    pub width: u32,
    pub height: u32,
}

impl<T: AsFd> ShmImage<T> {
    pub fn image(&self) -> anyhow::Result<image::RgbaImage> {
        let mmap = unsafe { memmap2::Mmap::map(&self.fd.as_fd())? };
        image::RgbaImage::from_raw(self.width, self.height, mmap.to_vec())
            .ok_or_else(|| anyhow::anyhow!("ShmImage had incorrect size"))
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!();
}

impl SeatHandler for AppData {
    fn seat_state(&mut self) -> &mut sctk::seat::SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl ToplevelManagerHandler for AppData {
    fn toplevel_manager_state(&mut self) -> &mut cctk::toplevel_management::ToplevelManagerState {
        &mut self.toplevel_manager_state
    }

    fn capabilities(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: Vec<WEnum<zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1>>,
    ) {
        // TODO capabilities could affect the options in the applet
    }
}
impl AppData {
    fn cosmic_toplevel(
        &self,
        handle: &ExtForeignToplevelHandleV1,
    ) -> Option<ZcosmicToplevelHandleV1> {
        self.toplevel_info_state
            .info(&handle)?
            .cosmic_toplevel
            .clone()
    }

    fn send_image(&self, handle: ExtForeignToplevelHandleV1) {
        let mut tx = self.tx.clone();
        let capure_data = CaptureData {
            qh: self.queue_handle.clone(),
            conn: self.conn.clone(),
            wl_shm: self.shm_state.wl_shm().clone(),
            capturer: self.screencopy_state.capturer().clone(),
        };
        std::thread::spawn(move || {
            use std::ffi::CStr;
            let name =
                unsafe { CStr::from_bytes_with_nul_unchecked(b"minimize-applet-screencopy\0") };
            let Ok(fd) = rustix::fs::memfd_create(name, rustix::fs::MemfdFlags::CLOEXEC) else {
                tracing::error!("Failed to get fd for capture");
                return;
            };

            // XXX is this going to use to much memory?
            let img = capure_data.capture_source_shm_fd(false, handle.clone(), fd, None);
            if let Some(img) = img {
                let Ok(mut img) = img.image() else {
                    tracing::error!("Failed to get RgbaImage");
                    return;
                };

                // resize to 128x128
                let max = img.width().max(img.height());
                let ratio = max as f32 / 128.0;

                if ratio > 1.0 {
                    let new_width = (img.width() as f32 / ratio).round();
                    let new_height = (img.height() as f32 / ratio).round();

                    img = image::imageops::resize(
                        &img,
                        new_width as u32,
                        new_height as u32,
                        image::imageops::FilterType::Nearest,
                    );
                }

                if let Err(err) = futures::executor::block_on(
                    tx.send(WaylandUpdate::Image(handle, WaylandImage::new(img))),
                ) {
                    tracing::error!("Failed to send image event to subscription {err:?}");
                };
            } else {
                tracing::error!("Failed to capture image");
            }
        });
    }
}

impl ToplevelInfoHandler for AppData {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            if info
                .state
                .contains(&zcosmic_toplevel_handle_v1::State::Minimized)
            {
                // spawn thread for sending the image
                self.send_image(toplevel.clone());
                let _ = futures::executor::block_on(
                    self.tx
                        .send(WaylandUpdate::Toplevel(ToplevelUpdate::Add(info.clone()))),
                );
            } else {
                let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(
                    ToplevelUpdate::Remove(toplevel.clone()),
                )));
            }
        }
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            if info
                .state
                .contains(&zcosmic_toplevel_handle_v1::State::Minimized)
            {
                self.send_image(toplevel.clone());
                let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(
                    ToplevelUpdate::Update(info.clone()),
                )));
            } else {
                let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(
                    ToplevelUpdate::Remove(toplevel.clone()),
                )));
            }
        }
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(
            ToplevelUpdate::Remove(toplevel.clone()),
        )));
    }
}

pub(crate) fn wayland_handler(
    tx: mpsc::Sender<WaylandUpdate>,
    rx: calloop::channel::Channel<WaylandRequest>,
) {
    let socket = std::env::var("X_PRIVILEGED_WAYLAND_SOCKET")
        .ok()
        .and_then(|fd| {
            fd.parse::<RawFd>()
                .ok()
                .map(|fd| unsafe { UnixStream::from_raw_fd(fd) })
        });

    let conn = if let Some(socket) = socket {
        Connection::from_socket(socket).unwrap()
    } else {
        Connection::connect_to_env().unwrap()
    };
    let (globals, event_queue) = registry_queue_init(&conn).unwrap();

    let mut event_loop = calloop::EventLoop::<AppData>::try_new().unwrap();
    let qh = event_queue.handle();
    let wayland_source = WaylandSource::new(conn.clone(), event_queue);
    let handle = event_loop.handle();
    wayland_source
        .insert(handle.clone())
        .expect("Failed to insert wayland source.");

    if handle
        .insert_source(rx, |event, _, state| match event {
            calloop::channel::Event::Msg(req) => match req {
                WaylandRequest::Toplevel(req) => match req {
                    ToplevelRequest::Activate(handle) => {
                        if let Some(seat) = state.seat_state.seats().next() {
                            let manager = &state.toplevel_manager_state.manager;
                            if let Some(cosmic_toplevel) = state.cosmic_toplevel(&handle) {
                                manager.activate(&cosmic_toplevel, &seat);
                            }
                        }
                    }
                },
            },
            calloop::channel::Event::Closed => {
                state.exit = true;
            }
        })
        .is_err()
    {
        return;
    }
    let registry_state = RegistryState::new(&globals);
    let screencopy_state = ScreencopyState::new(&globals, &qh);
    let shm_state = Shm::bind(&globals, &qh).expect("Failed to get shm state");

    let mut app_data = AppData {
        exit: false,
        tx,
        conn,
        queue_handle: qh.clone(),
        shm_state,
        screencopy_state,
        seat_state: SeatState::new(&globals, &qh),
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        registry_state,
    };

    loop {
        if app_data.exit {
            break;
        }
        event_loop.dispatch(None, &mut app_data).unwrap();
    }
}

impl ShmHandler for AppData {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl ScreencopyHandler for AppData {
    fn screencopy_state(&mut self) -> &mut ScreencopyState {
        &mut self.screencopy_state
    }

    fn init_done(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        session: &CaptureSession,
        formats: &Formats,
    ) {
        Session::for_session(session).unwrap().update(|data| {
            data.formats = Some(formats.clone());
        });
    }

    fn ready(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        screencopy_frame: &CaptureFrame,
        _frame: Frame,
    ) {
        let session = &screencopy_frame.data::<FrameData>().unwrap().session;
        Session::for_session(session).unwrap().update(|data| {
            data.res = Some(Ok(()));
        });
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        screencopy_frame: &CaptureFrame,
        reason: WEnum<FailureReason>,
    ) {
        // TODO send message to thread
        let session = &screencopy_frame.data::<FrameData>().unwrap().session;
        Session::for_session(session).unwrap().update(|data| {
            data.res = Some(Err(reason));
        });
    }

    fn stopped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _session: &CaptureSession) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        _buffer: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        _buffer: &wl_buffer::WlBuffer,
        _event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

sctk::delegate_shm!(AppData);
sctk::delegate_seat!(AppData);
sctk::delegate_registry!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_toplevel_manager!(AppData);
cctk::delegate_screencopy!(AppData, session: [SessionData], frame: [FrameData]);
