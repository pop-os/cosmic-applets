use async_io::Timer;
use futures_util::StreamExt;
use libpulse_binding::mainloop::standard::Mainloop;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub async fn drive_main_loop(main_loop: Rc<RefCell<Mainloop>>) {
    let mut timer = Timer::interval(Duration::from_millis(100));
    loop {
        main_loop.borrow_mut().iterate(false);
        timer.next().await;
    }
}
