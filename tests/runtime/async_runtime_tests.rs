use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc;
use std::task::{Context, Poll};
use std::thread;

use tsonic_rust_runtime::block_on;

#[test]
fn block_on_returns_ready_output() {
    assert_eq!(block_on(async { 42_i32 }), 42);
}

#[test]
fn block_on_parks_until_the_future_wakes_it() {
    let (sender, receiver) = mpsc::channel();
    assert_eq!(block_on(ThreadSignal::new(sender, receiver)), 7);
}

struct ThreadSignal {
    sender: Option<mpsc::Sender<i32>>,
    receiver: mpsc::Receiver<i32>,
}

impl ThreadSignal {
    fn new(sender: mpsc::Sender<i32>, receiver: mpsc::Receiver<i32>) -> Self {
        Self {
            sender: Some(sender),
            receiver,
        }
    }
}

impl Future for ThreadSignal {
    type Output = i32;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Ok(value) = self.receiver.try_recv() {
            return Poll::Ready(value);
        }
        if let Some(sender) = self.sender.take() {
            let waker = context.waker().clone();
            thread::spawn(move || {
                sender
                    .send(7)
                    .expect("receiver must remain live while blocked");
                waker.wake();
            });
        }
        Poll::Pending
    }
}
