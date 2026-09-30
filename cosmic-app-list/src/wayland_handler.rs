// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use crate::wayland_subscription::{
    OutputUpdate, ToplevelRequest, ToplevelUpdate, WaylandImage, WaylandRequest, WaylandUpdate,
};
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    os::{
        fd::{AsFd, FromRawFd, RawFd},
        unix::net::UnixStream,
    },
    sync::{Arc, Condvar, Mutex, MutexGuard, Weak},
    time::Duration,
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
    icon_captures: HashMap<ExtForeignToplevelHandleV1, u64>,
    capture_sessions: Arc<SessionRegistry>,
    icon_capture_scheduler: Arc<IconCaptureScheduler>,
}

impl Drop for AppData {
    fn drop(&mut self) {
        self.icon_capture_scheduler.stop();
        self.capture_sessions.cancel_all();
    }
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
        if let Some(info) = self.toplevel_info_state.info(toplevel).cloned() {
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Toplevel(ToplevelUpdate::Add(info.clone())));
            self.send_icon(&info);
        }
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel).cloned() {
            let _ = self
                .tx
                .unbounded_send(WaylandUpdate::Toplevel(ToplevelUpdate::Update(
                    info.clone(),
                )));
            self.send_icon(&info);
        }
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.icon_captures.remove(toplevel);
        self.icon_capture_scheduler.remove(toplevel);
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
    stopped: bool,
}

// TODO: dmabuf? need to handle modifier negotation
#[derive(Default)]
struct Session {
    condvar: Condvar,
    inner: Mutex<SessionInner>,
}

#[derive(Default)]
struct SessionRegistry {
    inner: Mutex<SessionRegistryInner>,
}

#[derive(Default)]
struct SessionRegistryInner {
    stopped: bool,
    sessions: Vec<Weak<Session>>,
}

impl SessionRegistry {
    fn register(&self, session: &Arc<Session>) -> bool {
        let stopped = {
            let mut registry = self.inner.lock().unwrap();
            if registry.stopped {
                true
            } else {
                registry
                    .sessions
                    .retain(|session| session.strong_count() > 0);
                registry.sessions.push(Arc::downgrade(session));
                false
            }
        };
        if stopped {
            session.update(|data| data.stopped = true);
            false
        } else {
            true
        }
    }

    fn cancel_all(&self) {
        let sessions = {
            let mut registry = self.inner.lock().unwrap();
            registry.stopped = true;
            registry
                .sessions
                .drain(..)
                .filter_map(|session| session.upgrade())
                .collect::<Vec<_>>()
        };
        for session in sessions {
            session.update(|data| data.stopped = true);
        }
    }
}

fn cancel_capture_sessions(registry: &SessionRegistry) {
    registry.cancel_all();
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

    fn wait_for_formats(&self) -> Option<Formats> {
        let mut data = self.wait_while(|data| data.formats.is_none() && !data.stopped);
        data.formats.take()
    }

    fn wait_for_formats_timeout(&self, timeout: Duration) -> Option<Formats> {
        let (mut data, _) = self
            .condvar
            .wait_timeout_while(self.inner.lock().unwrap(), timeout, |data| {
                data.formats.is_none() && !data.stopped
            })
            .unwrap();
        data.formats.take()
    }

    fn wait_for_result(&self) -> Option<Result<(), WEnum<FailureReason>>> {
        let mut data = self.wait_while(|data| data.res.is_none() && !data.stopped);
        data.res.take()
    }

    fn wait_for_result_timeout(
        &self,
        timeout: Duration,
    ) -> Option<Result<(), WEnum<FailureReason>>> {
        let (mut data, _) = self
            .condvar
            .wait_timeout_while(self.inner.lock().unwrap(), timeout, |data| {
                data.res.is_none() && !data.stopped
            })
            .unwrap();
        data.res.take()
    }

    fn stop(&self) {
        self.update(|data| data.stopped = true);
    }

    fn is_stopped(&self) -> bool {
        self.inner.lock().unwrap().stopped
    }
}

struct CoalescingCaptureQueue<K, V> {
    max_entries: usize,
    stopped: bool,
    pending_order: VecDeque<K>,
    pending: HashMap<K, V>,
    active: HashMap<K, Arc<Session>>,
}

impl<K: Clone + Eq + Hash, V> CoalescingCaptureQueue<K, V> {
    fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            stopped: false,
            pending_order: VecDeque::new(),
            pending: HashMap::new(),
            active: HashMap::new(),
        }
    }

    fn enqueue(&mut self, key: K, value: V) -> bool {
        if self.stopped {
            return false;
        }
        let known = self.pending.contains_key(&key) || self.active.contains_key(&key);
        if !known && self.len() >= self.max_entries {
            return false;
        }
        if let Some(session) = self.active.get(&key) {
            session.stop();
        }
        if self.pending.insert(key.clone(), value).is_none() {
            self.pending_order.push_back(key);
        }
        true
    }

    fn next(&mut self) -> Option<(K, V, Arc<Session>)> {
        while let Some(key) = self.pending_order.pop_front() {
            let Some(value) = self.pending.remove(&key) else {
                continue;
            };
            let session = Arc::new(Session::default());
            self.active.insert(key.clone(), session.clone());
            return Some((key, value, session));
        }
        None
    }

    fn finish(&mut self, key: &K, session: &Arc<Session>) {
        if self
            .active
            .get(key)
            .is_some_and(|active| Arc::ptr_eq(active, session))
        {
            self.active.remove(key);
        }
    }

    fn remove(&mut self, key: &K) {
        self.pending.remove(key);
        self.pending_order.retain(|pending| pending != key);
        if let Some(session) = self.active.remove(key) {
            session.stop();
        }
    }

    fn len(&self) -> usize {
        self.pending
            .keys()
            .filter(|key| !self.active.contains_key(*key))
            .count()
            + self.active.len()
    }

    fn stop(&mut self) {
        self.stopped = true;
        self.pending.clear();
        self.pending_order.clear();
        for (_, session) in self.active.drain() {
            session.stop();
        }
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
    sessions: Arc<SessionRegistry>,
}

const ICON_CAPTURE_WORKERS: usize = 4;
const MAX_ICON_CAPTURE_JOBS: usize = 64;
const ICON_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

struct IconCaptureJob {
    capture_data: CaptureData,
    source: CaptureSource,
    handle: ExtForeignToplevelHandleV1,
    generation: u64,
    tx: UnboundedSender<WaylandUpdate>,
}

struct IconCaptureScheduler {
    queue: Mutex<CoalescingCaptureQueue<ExtForeignToplevelHandleV1, IconCaptureJob>>,
    ready: Condvar,
}

impl IconCaptureScheduler {
    fn start() -> Arc<Self> {
        let scheduler = Arc::new(Self {
            queue: Mutex::new(CoalescingCaptureQueue::new(MAX_ICON_CAPTURE_JOBS)),
            ready: Condvar::new(),
        });
        for index in 0..ICON_CAPTURE_WORKERS {
            let scheduler = scheduler.clone();
            std::thread::Builder::new()
                .name(format!("app-list-icon-{index}"))
                .spawn(move || scheduler.run())
                .expect("failed to start icon capture worker");
        }
        scheduler
    }

    fn enqueue(&self, job: IconCaptureJob) -> bool {
        let handle = job.handle.clone();
        let queued = self.queue.lock().unwrap().enqueue(handle, job);
        if queued {
            self.ready.notify_one();
        }
        queued
    }

    fn remove(&self, handle: &ExtForeignToplevelHandleV1) {
        self.queue.lock().unwrap().remove(handle);
    }

    fn stop(&self) {
        self.queue.lock().unwrap().stop();
        self.ready.notify_all();
    }

    fn run(&self) {
        loop {
            let next = {
                let mut queue = self.queue.lock().unwrap();
                loop {
                    if let Some(next) = queue.next() {
                        break Some(next);
                    }
                    if queue.stopped {
                        break None;
                    }
                    queue = self.ready.wait(queue).unwrap();
                }
            };
            let Some((handle, job, session)) = next else {
                return;
            };

            if job.capture_data.sessions.register(&session) && !session.is_stopped() {
                capture_icon_job(job, session.clone());
            }
            self.queue.lock().unwrap().finish(&handle, &session);
        }
    }
}

fn shm_buffer_layout(width: u32, height: u32) -> Option<(i32, i32, i32, u32)> {
    const MAX_CAPTURE_PIXELS: u32 = 16 * 1024 * 1024;
    let pixels = width.checked_mul(height)?;
    if pixels == 0 || pixels > MAX_CAPTURE_PIXELS {
        return None;
    }
    let width_i32 = i32::try_from(width).ok()?;
    let height_i32 = i32::try_from(height).ok()?;
    let stride = width_i32.checked_mul(4)?;
    let len = pixels.checked_mul(4)?;
    i32::try_from(len).ok()?;
    Some((width_i32, height_i32, stride, len))
}

impl CaptureData {
    pub fn capture_source_shm_fd<Fd: AsFd>(
        &self,
        overlay_cursor: bool,
        source: &CaptureSource,
        fd: Fd,
        len: Option<u32>,
    ) -> Option<ShmImage<Fd>> {
        let session = Arc::new(Session::default());
        if !self.sessions.register(&session) {
            return None;
        }
        self.capture_source_shm_fd_with_session(overlay_cursor, source, fd, len, session, None)
    }

    fn capture_source_shm_fd_with_session<Fd: AsFd>(
        &self,
        overlay_cursor: bool,
        source: &CaptureSource,
        fd: Fd,
        len: Option<u32>,
        session: Arc<Session>,
        timeout: Option<Duration>,
    ) -> Option<ShmImage<Fd>> {
        // XXX error type?
        // TODO: way to get cursor metadata?

        #[allow(unused_variables)] // TODO
        let overlay_cursor = if overlay_cursor { 1 } else { 0 };

        if session.is_stopped() {
            return None;
        }
        let capture_session = match self.capturer.create_session(
            source,
            CaptureOptions::empty(),
            &self.qh,
            SessionData {
                session: session.clone(),
                session_data: ScreencopySessionData::default(),
            },
        ) {
            Ok(session) => session,
            Err(err) => {
                tracing::debug!(?err, "Image-copy capture is unavailable");
                return None;
            }
        };
        self.conn.flush().unwrap();

        let formats = if let Some(timeout) = timeout {
            session.wait_for_formats_timeout(timeout)?
        } else {
            session.wait_for_formats()?
        };
        let (width, height) = formats.buffer_size;
        let (width_i32, height_i32, stride_i32, buf_len) = shm_buffer_layout(width, height)?;

        // XXX
        let format = if formats.shm_formats.contains(&wl_shm::Format::Abgr8888) {
            wl_shm::Format::Abgr8888
        } else if formats.shm_formats.contains(&wl_shm::Format::Argb8888) {
            wl_shm::Format::Argb8888
        } else {
            tracing::error!("No suitable buffer format found");
            tracing::warn!("Available formats: {:#?}", formats);
            return None;
        };

        if let Some(len) = len {
            if len != buf_len {
                return None;
            }
        } else if let Err(err) = rustix::fs::ftruncate(&fd, u64::from(buf_len)) {
            tracing::error!(?err, "Failed to size screencopy buffer");
            return None;
        }
        let pool = self
            .wl_shm
            .create_pool(fd.as_fd(), i32::try_from(buf_len).ok()?, &self.qh, ());
        let buffer = pool.create_buffer(0, width_i32, height_i32, stride_i32, format, &self.qh, ());

        let frame = capture_session.capture(
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
        let res = if let Some(timeout) = timeout {
            session.wait_for_result_timeout(timeout)
        } else {
            session.wait_for_result()
        };
        frame.destroy();
        pool.destroy();
        buffer.destroy();
        if let Err(err) = self.conn.flush() {
            tracing::debug!(?err, "Failed to flush capture cleanup");
        }
        let res = res?;

        //std::thread::sleep(std::time::Duration::from_millis(16));

        if res.is_ok() {
            Some(ShmImage {
                fd,
                width,
                height,
                format,
            })
        } else {
            None
        }
    }
}

pub struct ShmImage<T: AsFd> {
    fd: T,
    pub width: u32,
    pub height: u32,
    format: wl_shm::Format,
}

fn normalize_shm_pixels(format: wl_shm::Format, pixels: &mut [u8]) -> Option<()> {
    if !matches!(format, wl_shm::Format::Abgr8888 | wl_shm::Format::Argb8888) {
        return None;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        let packed = u32::from_ne_bytes(pixel.try_into().ok()?);
        let alpha = (packed >> 24) as u8;
        let (red, green, blue) = if format == wl_shm::Format::Argb8888 {
            ((packed >> 16) as u8, (packed >> 8) as u8, packed as u8)
        } else {
            (packed as u8, (packed >> 8) as u8, (packed >> 16) as u8)
        };
        let unpremultiply = |channel: u8| {
            if alpha == 0 {
                0
            } else {
                (((u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255))
                    as u8
            }
        };
        pixel.copy_from_slice(&[
            unpremultiply(red),
            unpremultiply(green),
            unpremultiply(blue),
            alpha,
        ]);
    }
    Some(())
}

impl<T: AsFd> ShmImage<T> {
    pub fn image(&self) -> anyhow::Result<image::RgbaImage> {
        let mmap = unsafe { memmap2::Mmap::map(&self.fd.as_fd())? };
        let mut pixels = mmap.to_vec();
        normalize_shm_pixels(self.format, &mut pixels)
            .ok_or_else(|| anyhow::anyhow!("ShmImage had an unsupported format"))?;
        image::RgbaImage::from_raw(self.width, self.height, pixels)
            .ok_or_else(|| anyhow::anyhow!("ShmImage had incorrect size"))
    }
}

fn capture_icon_job(job: IconCaptureJob, session: Arc<Session>) {
    let Ok(fd) = rustix::fs::memfd_create(c"app-list-icon", rustix::fs::MemfdFlags::CLOEXEC) else {
        tracing::error!("Failed to get fd for icon capture");
        return;
    };
    let Some(img) = job.capture_data.capture_source_shm_fd_with_session(
        false,
        &job.source,
        fd,
        None,
        session,
        Some(ICON_CAPTURE_TIMEOUT),
    ) else {
        tracing::debug!("Toplevel icon capture was canceled or failed");
        return;
    };
    let Ok(img) = img.image() else {
        tracing::error!("Failed to decode captured toplevel icon");
        return;
    };
    if let Err(err) = job.tx.unbounded_send(WaylandUpdate::Icon(
        job.handle,
        job.generation,
        WaylandImage::new(img),
    )) {
        tracing::error!("Failed to send icon event to subscription {err:?}");
    }
}

fn mark_icon_capture_requested<K: Eq + Hash>(
    requested: &mut HashMap<K, u64>,
    key: K,
    generation: u64,
) -> bool {
    if requested.get(&key) == Some(&generation) {
        false
    } else {
        requested.insert(key, generation);
        true
    }
}

// Keep the current handle across same-generation metadata updates: it may be a
// successfully captured raster fallback rather than the locally resolved name.
pub(crate) fn should_replace_icon_handle(
    old_generation: u64,
    new_generation: u64,
    new_icon_is_present: bool,
) -> bool {
    !new_icon_is_present || old_generation != new_generation
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
            sessions: self.capture_sessions.clone(),
        };
        std::thread::spawn(move || {
            let name = c"app-list-screencopy";
            let Ok(fd) = rustix::fs::memfd_create(name, rustix::fs::MemfdFlags::CLOEXEC) else {
                tracing::error!("Failed to get fd for capture");
                return;
            };

            // XXX is this going to use to much memory?
            let img = capture_data.capture_source_shm_fd(
                false,
                &CaptureSource::Toplevel(handle.clone()),
                fd,
                None,
            );
            if let Some(img) = img {
                let Ok(img) = img.image() else {
                    tracing::error!("Failed to get RgbaImage");
                    return;
                };

                // resize to 256x256
                let max = img.width().max(img.height());
                let ratio = max as f32 / 256.0;

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

    fn send_icon(&mut self, info: &cctk::toplevel_info::ToplevelInfo) {
        let Some(icon) = info.icon.as_ref() else {
            self.icon_captures.remove(&info.foreign_toplevel);
            self.icon_capture_scheduler.remove(&info.foreign_toplevel);
            return;
        };
        if !mark_icon_capture_requested(
            &mut self.icon_captures,
            info.foreign_toplevel.clone(),
            info.icon_generation,
        ) {
            return;
        }
        let Ok(source) = icon.capture_source(256, 256) else {
            self.icon_captures.remove(&info.foreign_toplevel);
            return;
        };
        let capture_data = CaptureData {
            qh: self.queue_handle.clone(),
            conn: self.conn.clone(),
            wl_shm: self.shm_state.wl_shm().clone(),
            capturer: self.screencopy_state.capturer().clone(),
            sessions: self.capture_sessions.clone(),
        };
        if !self.icon_capture_scheduler.enqueue(IconCaptureJob {
            capture_data,
            source,
            handle: info.foreign_toplevel.clone(),
            generation: info.icon_generation,
            tx: self.tx.clone(),
        }) {
            self.icon_captures.remove(&info.foreign_toplevel);
            tracing::warn!("Toplevel icon capture queue is full or stopped");
        }
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

    fn stopped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, session: &CaptureSession) {
        if let Some(session) = Session::for_session(session) {
            session.update(|data| data.stopped = true);
        }
    }
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
        icon_captures: HashMap::new(),
        capture_sessions: Arc::new(SessionRegistry::default()),
        icon_capture_scheduler: IconCaptureScheduler::start(),
        queue_handle: qh,
    };

    loop {
        if app_data.exit {
            break;
        }
        if let Err(err) = event_loop.dispatch(None, &mut app_data) {
            tracing::error!(?err, "Wayland event dispatch failed");
            break;
        }
    }
    cancel_capture_sessions(&app_data.capture_sessions);
}

sctk::delegate_seat!(AppData);
sctk::delegate_registry!(AppData);
sctk::delegate_shm!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_workspace!(AppData);
cctk::delegate_toplevel_manager!(AppData);
cctk::delegate_screencopy!(AppData);

sctk::delegate_activation!(AppData, ExecRequestData);

sctk::delegate_output!(AppData);
