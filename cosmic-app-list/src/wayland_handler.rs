// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::wayland_subscription::{
    OutputUpdate, ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};
use std::{
    os::{
        fd::{AsFd, FromRawFd, RawFd},
        unix::net::UnixStream,
    },
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

use cctk::{
    screencopy::{
        CaptureFrame, CaptureOptions, CaptureSession, CaptureSource, Capturer, FailureReason,
        Formats, Frame, ScreencopyFrameData, ScreencopyFrameDataExt, ScreencopyHandler,
        ScreencopySessionData, ScreencopySessionDataExt, ScreencopyState,
    },
    sctk::{
        self,
        activation::{RequestData, RequestDataExt},
        output::{OutputHandler, OutputState},
        reexports::{calloop, calloop_wayland_source::WaylandSource},
        seat::{SeatHandler, SeatState},
        shm::{Shm, ShmHandler},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{
        Connection, Dispatch, QueueHandle, WEnum,
        globals::registry_queue_init,
        protocol::{
            wl_buffer, wl_output,
            wl_seat::WlSeat,
            wl_shm::{self, WlShm},
            wl_shm_pool,
            wl_surface::WlSurface,
        },
    },
    wayland_protocols::ext::{
        foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
        workspace::v1::client::ext_workspace_handle_v1::State as WorkspaceUpdateState,
    },
    workspace::{WorkspaceHandler, WorkspaceState},
};
use cosmic_protocols::{
    toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
};
use futures::channel::mpsc::UnboundedSender;
use sctk::{
    activation::{ActivationHandler, ActivationState},
    registry::{ProvidesRegistryState, RegistryState},
};
struct AppData {
    exit: bool,
    tx: UnboundedSender<WaylandUpdate>,
    conn: Connection,
    queue_handle: QueueHandle<Self>,
    output_state: OutputState,
    workspace_state: WorkspaceState,
    toplevel_info_state: ToplevelInfoState,
    toplevel_manager_state: ToplevelManagerState,
    screencopy_state: ScreencopyState,
    registry_state: RegistryState,
    seat_state: SeatState,
    shm_state: Shm,
    activation_state: Option<ActivationState>,
}

// Workspace and toplevel handling

// Need to bind output globals just so workspace can get output events
impl OutputHandler for AppData {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some(info) = self.output_state.info(&output) {
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Output(OutputUpdate::Add(output, info)));
        }
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        if let Some(info) = self.output_state.info(&output) {
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Output(OutputUpdate::Update(output, info)));
        }
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let _ = self
            .tx
            .unbounded_send(WaylandUpdate::Output(OutputUpdate::Remove(output)));
    }
}

impl WorkspaceHandler for AppData {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }

    fn done(&mut self) {
        let active_workspaces = self
            .workspace_state
            .workspace_groups()
            .filter_map(|x| {
                x.workspaces
                    .iter()
                    .filter_map(|handle| self.workspace_state.workspace_info(handle))
                    .find(|w| w.state.contains(WorkspaceUpdateState::Active))
                    .map(|workspace| workspace.handle.clone())
            })
            .collect::<Vec<_>>();
        let _ = self
            .tx
            .unbounded_send(WaylandUpdate::Workspace(active_workspaces));
    }
}

impl ProvidesRegistryState for AppData {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!();
}

struct ExecRequestData {
    data: RequestData,
    exec: String,
    gpu_idx: Option<usize>,
    terminal: bool,
}

impl RequestDataExt for ExecRequestData {
    fn app_id(&self) -> Option<&str> {
        self.data.app_id()
    }

    fn seat_and_serial(&self) -> Option<(&WlSeat, u32)> {
        self.data.seat_and_serial()
    }

    fn surface(&self) -> Option<&WlSurface> {
        self.data.surface()
    }
}

impl ActivationHandler for AppData {
    type RequestData = ExecRequestData;
    fn new_token(&mut self, token: String, data: &ExecRequestData) {
        let _ = self.tx.unbounded_send(WaylandUpdate::ActivationToken {
            token: Some(token),
            app_id: data.app_id().map(String::from),
            exec: data.exec.clone(),
            gpu_idx: data.gpu_idx,
            terminal: data.terminal,
        });
    }
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
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Toplevel(ToplevelUpdate::Add(info.clone())));
        }
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Toplevel(ToplevelUpdate::Update(
                    info.clone(),
                )));
        }
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        let _ = self
            .tx
            .unbounded_send(WaylandUpdate::Toplevel(ToplevelUpdate::Remove(
                toplevel.clone(),
            )));
    }
}

// Screencopy handling

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

    fn wait_while<F: FnMut(&SessionInner) -> bool>(
        &self,
        mut f: F,
    ) -> MutexGuard<'_, SessionInner> {
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

impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppData {
    fn event(
        _app_data: &mut Self,
        _buffer: &wl_shm_pool::WlShmPool,
        _event: wl_shm_pool::Event,
        (): &(),
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
        (): &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
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
        source: &ExtForeignToplevelHandleV1,
        fd: Fd,
        len: Option<u32>,
    ) -> Option<ShmImage<Fd>> {
        // XXX error type?
        // TODO: way to get cursor metadata?

        #[allow(unused_variables)] // TODO
        let overlay_cursor = if overlay_cursor { 1 } else { 0 };

        let session = Arc::new(Session::default());
        // Unwrap assumes compositor supports this capture type
        let capture_session = self
            .capturer
            .create_session(
                &CaptureSource::Toplevel(source.clone()),
                CaptureOptions::empty(),
                &self.qh,
                SessionData {
                    session: session.clone(),
                    session_data: ScreencopySessionData::default(),
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
        if !formats.shm_formats.contains(&wl_shm::Format::Abgr8888) {
            tracing::error!("No suitable buffer format found");
            tracing::warn!("Available formats: {:#?}", formats);
            return None;
        }

        let buf_len = width * height * 4;
        if let Some(len) = len {
            if len != buf_len {
                return None;
            }
        } else if let Err(_err) = rustix::fs::ftruncate(&fd, buf_len.into()) {
        }
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
                frame_data: ScreencopyFrameData::default(),
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

impl AppData {
    fn cosmic_toplevel(
        &self,
        handle: &ExtForeignToplevelHandleV1,
    ) -> Option<ZcosmicToplevelHandleV1> {
        self.toplevel_info_state
            .info(handle)?
            .cosmic_toplevel
            .clone()
    }

    fn send_image(&self, handle: ExtForeignToplevelHandleV1) {
        let tx = self.tx.clone();
        let capture_data = CaptureData {
            qh: self.queue_handle.clone(),
            conn: self.conn.clone(),
            wl_shm: self.shm_state.wl_shm().clone(),
            capturer: self.screencopy_state.capturer().clone(),
        };
        std::thread::spawn(move || {
            let name = c"app-list-screencopy";
            let Ok(fd) = rustix::fs::memfd_create(name, rustix::fs::MemfdFlags::CLOEXEC) else {
                tracing::error!("Failed to get fd for capture");
                return;
            };

            // XXX is this going to use to much memory?
            let img = capture_data.capture_source_shm_fd(false, &handle, fd, None);
            if let Some(img) = img {
                let Ok(img) = img.image() else {
                    tracing::error!("Failed to get RgbaImage");
                    return;
                };

                // resize to 128x128
                let max = img.width().max(img.height());
                let ratio = max as f32 / 128.0;

                let img = if ratio > 1.0 {
                    let new_width = (img.width() as f32 / ratio).round();
                    let new_height = (img.height() as f32 / ratio).round();

                    image::imageops::resize(
                        &img,
                        new_width as u32,
                        new_height as u32,
                        image::imageops::FilterType::Lanczos3,
                    )
                } else {
                    img
                };

                if let Err(err) =
                    tx.unbounded_send(WaylandUpdate::Image(handle, WaylandImage::new(img)))
                {
                    tracing::error!("Failed to send image event to subscription {err:?}");
                }
            } else {
                tracing::error!("Failed to capture image");
            }
        });
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

pub(crate) fn wayland_handler(
    tx: UnboundedSender<WaylandUpdate>,
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
        .insert_source(rx, |event, (), state| match event {
            calloop::channel::Event::Msg(req) => match req {
                WaylandRequest::Screencopy(handle) => {
                    state.send_image(handle.clone());
                }
                WaylandRequest::Toplevel(req) => match req {
                    ToplevelRequest::Activate(handle) => {
                        if let Some(cosmic_toplevel) = state.cosmic_toplevel(&handle) {
                            if let Some(seat) = state.seat_state.seats().next() {
                                let manager = &state.toplevel_manager_state.manager;
                                manager.activate(&cosmic_toplevel, &seat);
                            }
                        }
                    }
                    ToplevelRequest::Minimize(handle) => {
                        if let Some(cosmic_toplevel) = state.cosmic_toplevel(&handle) {
                            let manager = &state.toplevel_manager_state.manager;
                            manager.set_minimized(&cosmic_toplevel);
                        }
                    }
                    ToplevelRequest::Quit(handle) => {
                        if let Some(cosmic_toplevel) = state.cosmic_toplevel(&handle) {
                            let manager = &state.toplevel_manager_state.manager;
                            manager.close(&cosmic_toplevel);
                        }
                    }
                },
                WaylandRequest::TokenRequest {
                    app_id,
                    exec,
                    gpu_idx,
                    terminal,
                } => {
                    if let Some(activation_state) = state.activation_state.as_ref() {
                        activation_state.request_token_with_data(
                            &state.queue_handle,
                            ExecRequestData {
                                data: RequestData {
                                    app_id: Some(app_id),
                                    seat_and_serial: state
                                        .seat_state
                                        .seats()
                                        .next()
                                        .map(|seat| (seat, 0)),
                                    surface: None,
                                },
                                exec,
                                gpu_idx,
                                terminal,
                            },
                        );
                    } else {
                        let _ = state.tx.unbounded_send(WaylandUpdate::ActivationToken {
                            token: None,
                            app_id: Some(app_id),
                            exec,
                            gpu_idx,
                            terminal,
                        });
                    }
                }
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

    let mut app_data = AppData {
        exit: false,
        tx,
        conn,
        output_state: OutputState::new(&globals, &qh),
        workspace_state: WorkspaceState::new(&registry_state, &qh),
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        screencopy_state: ScreencopyState::new(&globals, &qh),
        registry_state,
        seat_state: SeatState::new(&globals, &qh),
        shm_state: Shm::bind(&globals, &qh).unwrap(),
        activation_state: ActivationState::bind::<AppData>(&globals, &qh).ok(),
        queue_handle: qh,
    };

    loop {
        if app_data.exit {
            break;
        }
        event_loop.dispatch(None, &mut app_data).unwrap();
    }
}

sctk::delegate_seat!(AppData);
sctk::delegate_registry!(AppData);
sctk::delegate_shm!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_workspace!(AppData);
cctk::delegate_toplevel_manager!(AppData);
cctk::delegate_screencopy!(AppData, session: [SessionData], frame: [FrameData]);

sctk::delegate_activation!(AppData, ExecRequestData);

sctk::delegate_output!(AppData);
