#![allow(missing_docs)]

use std::time::Duration;

use bytes::BytesMut;
use bytestring::ByteString;
use divan::{AllocProfiler, Bencher, black_box};
use russe::{Encoder, Event, Message};
use tokio_util::codec::Encoder as _;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn data_message(data: impl Into<ByteString>) -> Message {
    Message {
        data: data.into(),
        event: None,
        retry: None,
        id: None,
    }
}

fn bench_event(b: Bencher<'_, '_>, event: Event, expected: &str) {
    assert_eq!(event.clone().into_bytestring().unwrap(), expected);

    b.with_inputs(|| event.clone())
        .bench_values(|event| event.into_bytestring().unwrap());
}

#[divan::bench(args = [32, 1024, 65536])]
fn message(b: Bencher<'_, '_>, size: usize) {
    let data = "x".repeat(size);
    let expected = format!("data: {data}\n\n");

    bench_event(b, Event::Message(data_message(data)), &expected);
}

#[divan::bench]
fn multiline_message(b: Bencher<'_, '_>) {
    let expected = format!(
        "id: 42\nevent: update\nretry: 1500\n{}\n",
        "data: café\ndata: 世界\ndata: \ndata: next\n".repeat(64) + "data: \n",
    );

    bench_event(
        b,
        Event::Message(Message {
            data: "café\r\n世界\n\nnext\r".repeat(64).into(),
            event: Some("update".into()),
            retry: Some(Duration::from_millis(1500)),
            id: Some("42".into()),
        }),
        &expected,
    );
}

#[divan::bench]
fn comment(b: Bencher<'_, '_>) {
    bench_event(b, Event::Comment("keep-alive".into()), ": keep-alive\n\n");
}

#[divan::bench]
fn retry(b: Bencher<'_, '_>) {
    bench_event(
        b,
        Event::Retry(Duration::from_millis(1500)),
        "retry: 1500\n\n",
    );
}

#[divan::bench(args = [32, 1024, 65536])]
fn codec_message(b: Bencher<'_, '_>, size: usize) {
    let data = "x".repeat(size);
    let expected = format!("data: {data}\n\n");
    let event = Event::Message(data_message(data));
    let mut output = BytesMut::new();

    Encoder::default()
        .encode(event.clone(), &mut output)
        .unwrap();
    assert_eq!(output.as_ref(), expected.as_bytes());

    b.with_inputs(|| event.clone()).bench_values(|event| {
        let mut output = BytesMut::new();

        Encoder::default().encode(event, &mut output).unwrap();

        output
    });
}

#[divan::bench(args = [32, 1024, 65536])]
fn codec_reused_buffer(b: Bencher<'_, '_>, size: usize) {
    let data = "x".repeat(size);
    let expected = format!("data: {data}\n\n");
    let event = Event::Message(data_message(data));
    let mut output = BytesMut::with_capacity(expected.len());
    let mut encoder = Encoder::default();

    encoder.encode(event.clone(), &mut output).unwrap();
    assert_eq!(output.as_ref(), expected.as_bytes());

    b.with_inputs(|| event.clone()).bench_local_values(|event| {
        output.clear();
        encoder.encode(event, &mut output).unwrap();
        black_box(output.as_ref());

        output.len()
    });
}

fn main() {
    divan::main();
}
