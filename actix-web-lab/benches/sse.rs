#![allow(missing_docs)]

use std::{fmt::Debug, pin::pin, time::Duration};

use actix_web::body::MessageBody;
use actix_web_lab::sse::{Data, Event, Sse};
use divan::{AllocProfiler, Bencher, black_box};
use futures_test::stream::StreamTestExt as _;
use futures_util::{future::poll_fn, stream};
use tokio::sync::mpsc;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

async fn drain_body<B>(body: B) -> (usize, usize)
where
    B: MessageBody,
    B::Error: Debug,
{
    let mut body = pin!(body);
    let mut chunks = 0;
    let mut bytes = 0;

    while let Some(chunk) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        let chunk = black_box(chunk.unwrap());

        chunks += 1;
        bytes += chunk.len();
    }

    (chunks, bytes)
}

async fn validate_body<B>(body: B, expected: &[u8], count: usize)
where
    B: MessageBody,
    B::Error: Debug,
{
    let mut body = pin!(body);
    let mut chunks = 0;

    while let Some(chunk) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
        assert_eq!(chunk.unwrap().as_ref(), expected);

        chunks += 1;
    }

    assert_eq!(chunks, count);
}

fn message(size: usize) -> (Event, String) {
    let data = "x".repeat(size);
    let expected = format!("id: 42\nevent: update\ndata: {data}\n\n");
    let event = Data::new(data).id("42").event("update").into();

    (event, expected)
}

#[divan::bench(args = [(32, 1), (1024, 1), (65536, 1), (32, 128), (1024, 128)])]
fn stream_body(b: Bencher<'_, '_>, (size, count): (usize, usize)) {
    let rt = tokio::runtime::Handle::current();
    let (event, expected) = message(size);
    let make_body = || Sse::from_infallible_stream(stream::iter(vec![event.clone(); count]));

    rt.block_on(validate_body(make_body(), expected.as_bytes(), count));

    b.with_inputs(make_body)
        .bench_values(|body| rt.block_on(drain_body(body)));
}

#[divan::bench(args = [(32, 1), (1024, 1), (65536, 1), (32, 128), (1024, 128)])]
fn receiver_body(b: Bencher<'_, '_>, (size, count): (usize, usize)) {
    let rt = tokio::runtime::Handle::current();
    let (event, expected) = message(size);
    let make_body = || {
        let (sender, receiver) = mpsc::channel(count);

        for _ in 0..count {
            sender.try_send(event.clone()).unwrap();
        }

        Sse::from_infallible_receiver(receiver)
    };

    rt.block_on(validate_body(make_body(), expected.as_bytes(), count));

    // Queue setup is excluded. Measure dequeueing and response body encoding.
    b.with_inputs(make_body)
        .bench_values(|body| rt.block_on(drain_body(body)));
}

#[divan::bench(args = [1, 128])]
fn pending_stream_body(b: Bencher<'_, '_>, count: usize) {
    let rt = tokio::runtime::Handle::current();
    let (event, expected) = message(32);
    let make_body = || {
        Sse::from_infallible_stream(stream::iter(vec![event.clone(); count]).interleave_pending())
    };

    rt.block_on(validate_body(make_body(), expected.as_bytes(), count));

    b.with_inputs(make_body)
        .bench_values(|body| rt.block_on(drain_body(body)));
}

#[divan::bench]
fn retry_body(b: Bencher<'_, '_>) {
    let rt = tokio::runtime::Handle::current();
    let make_body = || {
        Sse::from_infallible_stream(stream::empty::<Event>())
            .with_retry_duration(Duration::from_millis(1500))
    };

    rt.block_on(validate_body(make_body(), b"retry: 1500\n\n", 1));

    b.with_inputs(make_body)
        .bench_values(|body| rt.block_on(drain_body(body)));
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    divan::main();
}
