#![feature(test)]

extern crate futures;
extern crate test;

use futures::prelude::*;
use futures::task::{self, Waker, Wake};
use futures::executor::LocalPool;

use test::Bencher;

fn notify_noop() -> Waker {
    struct Noop;

    impl Wake for Noop {
        fn wake(&self) {}
    }

    const NOOP : &'static Noop = &Noop;

    Waker::from(NOOP)
}

#[bench]
fn task_init(b: &mut Bencher) {
    const NUM: u32 = 100_000;

    struct MyFuture {
        num: u32,
        task: Option<Waker>,
    };

    impl Future for MyFuture {
        type Item = ();
        type Error = ();

        fn poll(&mut self, cx: &mut task::Context) -> Poll<(), ()> {
            if self.num == NUM {
                Ok(Async::Ready(()))
            } else {
                self.num += 1;

                if let Some(ref t) = self.task {
                    t.wake();
                    return Ok(Async::Pending);
                }

                let t = cx.waker();
                t.wake();
                self.task = Some(t);

                Ok(Async::Pending)
            }
        }
    }

    let mut fut = MyFuture {
        num: 0,
        task: None,
    };

    let pool = LocalPool::new();
    let mut exec = pool.executor();
    let waker = notify_noop();
    let mut map = task::LocalMap::new();
    let mut cx = task::Context::new(&mut map, &waker, &mut exec);

    b.iter(|| {
        fut.num = 0;

        while let Ok(Async::Pending) = fut.poll(&mut cx) {
        }
    });
}
