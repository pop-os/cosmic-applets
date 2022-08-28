use libpulse_binding::operation::Operation;
use std::{
    cell::RefCell,
    future::{self, Future},
    pin::Pin,
    rc::Rc,
    task::{self, Poll, Waker},
};

struct PAFutInner<T> {
    res: Option<T>,
    waker: Option<Waker>,
}

pub struct PAFutWaker<T>(Rc<RefCell<PAFutInner<T>>>);

impl<T> PAFutWaker<T> {
    pub fn wake(&self, res: T) {
        let mut inner = self.0.borrow_mut();
        inner.res = Some(res);
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

pub struct PAFut<T, F: ?Sized> {
    inner: Rc<RefCell<PAFutInner<T>>>,
    operation: Operation<F>,
}

impl<T, F: ?Sized> PAFut<T, F> {
    pub fn new(cb: impl FnOnce(PAFutWaker<T>) -> Operation<F>) -> Self {
        let inner = Rc::new(RefCell::new(PAFutInner {
            res: None,
            waker: None,
        }));
        let operation = cb(PAFutWaker(inner.clone()));
        Self { inner, operation }
    }
}

impl<T, F: ?Sized> Future for PAFut<T, F> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();
        if let Some(res) = inner.res.take() {
            Poll::Ready(res)
        } else {
            inner.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T, F: ?Sized> Drop for PAFut<T, F> {
    fn drop(&mut self) {
        self.operation.cancel();
    }
}
