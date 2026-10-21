use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::Mutex;
use std::task::{Poll, Waker};

#[derive(Default)]
pub struct Signal {
    state: Mutex<Raised>,
}

#[derive(Default)]
struct Raised {
    raised: bool,
    waker: Option<Waker>,
}

impl Signal {
    pub fn raise(&self) {
        let waker = match self.state.lock() {
            Ok(mut state) => {
                state.raised = true;
                state.waker.take()
            }
            Err(_) => None,
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub fn reset(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.raised = false;
        }
    }

    pub async fn wait(&self) {
        poll_fn(|cx| {
            let Ok(mut state) = self.state.lock() else {
                return Poll::Ready(());
            };
            if state.raised {
                state.raised = false;
                return Poll::Ready(());
            }
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }
}

pub enum First<A, B> {
    Left(A),
    Right(B),
}

pub async fn first<A: Future, B: Future>(left: A, right: B) -> First<A::Output, B::Output> {
    let mut left = pin!(left);
    let mut right = pin!(right);
    poll_fn(|cx| {
        if let Poll::Ready(value) = left.as_mut().poll(cx) {
            return Poll::Ready(First::Left(value));
        }
        if let Poll::Ready(value) = right.as_mut().poll(cx) {
            return Poll::Ready(First::Right(value));
        }
        Poll::Pending
    })
    .await
}
