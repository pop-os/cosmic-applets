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
    wayland_client::{self, WEnum, delegate_noop, protocol::wl_seat::WlSeat},
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
    iced::futures,
};
use cosmic_protocols::{
    toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
};
use futures::{SinkExt, channel::mpsc};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use wayland_client::{Connection, QueueHandle, globals::registry_queue_init};

const THUMBNAIL_MAX_EDGE: u32 = 128;

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

    fn stop(&self) {
        self.update(|data| data.stopped = true);
    }

    fn wait_for_formats(&self) -> Option<Formats> {
        self.wait_for_formats_locked(self.inner.lock().unwrap())
    }

    fn wait_for_formats_locked(&self, data: MutexGuard<'_, SessionInner>) -> Option<Formats> {
        let mut data = self.wait_while(data, |data| data.formats.is_none() && !data.stopped);
        if data.stopped {
            None
        } else {
            data.formats.take()
        }
    }

    fn wait_for_result(&self) -> Option<Result<(), WEnum<FailureReason>>> {
        self.wait_for_result_locked(self.inner.lock().unwrap())
    }

    fn wait_for_result_locked(
        &self,
        data: MutexGuard<'_, SessionInner>,
    ) -> Option<Result<(), WEnum<FailureReason>>> {
        self.wait_while(data, |data| data.res.is_none() && !data.stopped)
            .res
            .take()
    }

    fn wait_while<'a, F: FnMut(&SessionInner) -> bool>(
        &self,
        data: MutexGuard<'a, SessionInner>,
        mut f: F,
    ) -> MutexGuard<'a, SessionInner> {
        self.condvar.wait_while(data, |data| f(data)).unwrap()
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
    captured_toplevels: std::collections::HashSet<ExtForeignToplevelHandleV1>,
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
                    session_data: ScreencopySessionData::default(),
                },
            )
            .unwrap();
        self.conn.flush().unwrap();

        let formats = session.wait_for_formats()?;
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
        let res = session.wait_for_result();
        pool.destroy();
        buffer.destroy();

        //std::thread::sleep(std::time::Duration::from_millis(16));

        if res?.is_ok() {
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
    pub fn image(&self) -> anyhow::Result<image::ImageBuffer<image::Rgba<u8>, memmap2::Mmap>> {
        let mmap = unsafe { memmap2::Mmap::map(&self.fd.as_fd())? };
        image::ImageBuffer::from_raw(self.width, self.height, mmap)
            .ok_or_else(|| anyhow::anyhow!("ShmImage had incorrect size"))
    }
}

fn make_thumbnail<I>(image: &I) -> image::RgbaImage
where
    I: image::GenericImageView<Pixel = image::Rgba<u8>>,
{
    if image.width() == 0 || image.height() == 0 {
        return image::RgbaImage::new(0, 0);
    }

    let max = image.width().max(image.height());
    let ratio = max as f32 / THUMBNAIL_MAX_EDGE as f32;
    let (width, height) = if ratio > 1.0 {
        (
            (image.width() as f32 / ratio).round() as u32,
            (image.height() as f32 / ratio).round() as u32,
        )
    } else {
        (image.width(), image.height())
    };

    let x_ratio = image.width() as f32 / width as f32;
    let y_ratio = image.height() as f32 / height as f32;
    image::RgbaImage::from_fn(width, height, |x, y| {
        let source_x = ((x as f32 + 0.5) * x_ratio).floor() as u32;
        let source_y = ((y as f32 + 0.5) * y_ratio).floor() as u32;
        image.get_pixel(
            source_x.min(image.width() - 1),
            source_y.min(image.height() - 1),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn large_capture_stays_mapped_and_produces_a_bounded_thumbnail() {
        const WIDTH: u32 = 1920;
        const HEIGHT: u32 = 1080;
        let len = u64::from(WIDTH) * u64::from(HEIGHT) * 4;
        let fd = rustix::fs::memfd_create(
            c"cosmic-minimize-thumbnail-test",
            rustix::fs::MemfdFlags::CLOEXEC,
        )
        .unwrap();
        rustix::fs::ftruncate(&fd, len).unwrap();

        let image = ShmImage {
            fd,
            width: WIDTH,
            height: HEIGHT,
        }
        .image()
        .unwrap();

        fn assert_mapped(_: &image::ImageBuffer<image::Rgba<u8>, memmap2::Mmap>) {}
        assert_mapped(&image);

        let thumbnail = make_thumbnail(&image);
        assert_eq!(thumbnail.dimensions(), (128, 72));
        assert!(thumbnail.as_raw().len() <= (THUMBNAIL_MAX_EDGE.pow(2) * 4) as usize);
    }

    #[test]
    fn stopped_session_releases_formats_waiter() {
        let session = Arc::new(Session::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let waiter = {
            let session = session.clone();
            std::thread::spawn(move || {
                let data = session.inner.lock().unwrap();
                entered_tx.send(()).unwrap();
                done_tx
                    .send(session.wait_for_formats_locked(data).is_none())
                    .unwrap();
            })
        };

        entered_rx.recv_timeout(WAIT_TIMEOUT).unwrap();
        session.stop();
        assert!(done_rx.recv_timeout(WAIT_TIMEOUT).unwrap());
        waiter.join().unwrap();
    }

    #[test]
    fn stopped_session_releases_result_waiter() {
        let session = Arc::new(Session::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let waiter = {
            let session = session.clone();
            std::thread::spawn(move || {
                let data = session.inner.lock().unwrap();
                entered_tx.send(()).unwrap();
                done_tx
                    .send(session.wait_for_result_locked(data).is_none())
                    .unwrap();
            })
        };

        entered_rx.recv_timeout(WAIT_TIMEOUT).unwrap();
        session.stop();
        assert!(done_rx.recv_timeout(WAIT_TIMEOUT).unwrap());
        waiter.join().unwrap();
    }

    #[test]
    fn completed_result_wins_if_session_then_stops() {
        let session = Session::default();
        session.update(|data| {
            data.res = Some(Ok(()));
            data.stopped = true;
        });

        assert!(matches!(session.wait_for_result(), Some(Ok(()))));
    }

    #[test]
    fn thumbnail_preserves_nearest_neighbor_output() {
        // 62 / 7 exposes a one-pixel difference if the resize ratio is not
        // rounded in the same order as image::imageops::resize.
        let image = image::RgbaImage::from_fn(62, 1_134, |x, y| {
            image::Rgba([
                x.wrapping_mul(17) as u8,
                y.wrapping_mul(29) as u8,
                x.wrapping_add(y) as u8,
                255,
            ])
        });
        let expected =
            image::imageops::resize(&image, 7, 128, image::imageops::FilterType::Nearest);

        assert_eq!(make_thumbnail(&image), expected);
        assert_eq!(
            make_thumbnail(&image::RgbaImage::new(0, 0)).dimensions(),
            (0, 0)
        );
        let thin = image::RgbaImage::new(1, 1_000);
        let thin_expected =
            image::imageops::resize(&thin, 0, 128, image::imageops::FilterType::Nearest);
        assert_eq!(make_thumbnail(&thin), thin_expected);
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
            .info(handle)?
            .cosmic_toplevel
            .clone()
    }

    fn send_image(&self, handle: ExtForeignToplevelHandleV1) {
        let mut tx = self.tx.clone();
        let capture_data = CaptureData {
            qh: self.queue_handle.clone(),
            conn: self.conn.clone(),
            wl_shm: self.shm_state.wl_shm().clone(),
            capturer: self.screencopy_state.capturer().clone(),
        };
        std::thread::spawn(move || {
            let name = c"minimize-applet-screencopy";
            let Ok(fd) = rustix::fs::memfd_create(name, rustix::fs::MemfdFlags::CLOEXEC) else {
                tracing::error!("Failed to get fd for capture");
                return;
            };

            let image = capture_data.capture_source_shm_fd(false, handle.clone(), fd, None);
            if let Some(shm_image) = image {
                let Ok(mapped_image) = shm_image.image() else {
                    tracing::error!("Failed to get RgbaImage");
                    return;
                };

                drop(shm_image);
                let thumbnail = make_thumbnail(&mapped_image);
                drop(mapped_image);

                if let Err(err) = futures::executor::block_on(
                    tx.send(WaylandUpdate::Image(handle, WaylandImage::new(thumbnail))),
                ) {
                    tracing::error!("Failed to send image event to subscription {err:?}");
                }
            } else {
                tracing::error!("Failed to capture image");
            }
        });
    }

    fn handle_toplevel(&mut self, toplevel: &ExtForeignToplevelHandleV1, is_new: bool) {
        let Some(info) = self.toplevel_info_state.info(toplevel) else {
            return;
        };
        if info
            .state
            .contains(&zcosmic_toplevel_handle_v1::State::Minimized)
        {
            // Capture a thumbnail once, on the transition into minimized.
            if self.captured_toplevels.insert(toplevel.clone()) {
                self.send_image(toplevel.clone());
            }
            let update = if is_new {
                ToplevelUpdate::Add(info.clone())
            } else {
                ToplevelUpdate::Update(info.clone())
            };
            let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(update)));
        } else {
            self.remove_toplevel(toplevel);
        }
    }

    fn remove_toplevel(&mut self, toplevel: &ExtForeignToplevelHandleV1) {
        self.captured_toplevels.remove(toplevel);
        let _ = futures::executor::block_on(self.tx.send(WaylandUpdate::Toplevel(
            ToplevelUpdate::Remove(toplevel.clone()),
        )));
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
        self.handle_toplevel(toplevel, true);
    }

    fn update_toplevel(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.handle_toplevel(toplevel, false);
    }

    fn toplevel_closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.remove_toplevel(toplevel);
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
        .insert_source(rx, |event, (), state| match event {
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
        shm_state,
        screencopy_state,
        seat_state: SeatState::new(&globals, &qh),
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &qh),
        toplevel_manager_state: ToplevelManagerState::new(&registry_state, &qh),
        queue_handle: qh,
        registry_state,
        captured_toplevels: std::collections::HashSet::new(),
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

    fn stopped(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, session: &CaptureSession) {
        Session::for_session(session).unwrap().stop();
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

sctk::delegate_shm!(AppData);
sctk::delegate_seat!(AppData);
sctk::delegate_registry!(AppData);
cctk::delegate_toplevel_info!(AppData);
cctk::delegate_toplevel_manager!(AppData);
cctk::delegate_screencopy!(AppData);
delegate_noop!(AppData: ignore wl_buffer::WlBuffer);
