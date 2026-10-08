#![allow(missing_docs)]

use std::time::Duration;

use divan::{AllocProfiler, Bencher};
use russe::{Event, Message};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn bench_event(b: Bencher<'_, '_>, event: Event) {
    b.with_inputs(|| event.clone())
        .bench_values(|event| event.into_bytestring().unwrap());
}

#[divan::bench(args = [32, 1024, 65536])]
fn message(b: Bencher<'_, '_>, size: usize) {
    bench_event(
        b,
        Event::Message(Message {
            data: "x".repeat(size).into(),
            event: None,
            retry: None,
            id: None,
        }),
    );
}

#[divan::bench]
fn multiline_message(b: Bencher<'_, '_>) {
    bench_event(
        b,
        Event::Message(Message {
            data: "café\r\n世界\n\nnext\r".repeat(64).into(),
            event: Some("update".into()),
            retry: Some(Duration::from_millis(1500)),
            id: Some("42".into()),
        }),
    );
}

#[divan::bench]
fn comment(b: Bencher<'_, '_>) {
    bench_event(b, Event::Comment("keep-alive".into()));
}

#[divan::bench]
fn retry(b: Bencher<'_, '_>) {
    bench_event(b, Event::Retry(Duration::from_millis(1500)));
}

fn main() {
    divan::main();
}
