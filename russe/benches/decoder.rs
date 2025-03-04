#![allow(missing_docs)]

use std::{hint::black_box, io, pin::pin};

use bytes::Bytes;
use divan::{AllocProfiler, Bencher};
use futures_test::stream::StreamTestExt as _;
use futures_util::{stream, StreamExt as _};
use tokio_util::{codec::FramedRead, io::StreamReader};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

#[divan::bench]
fn sse_events(b: Bencher<'_, '_>) {
    let rt = tokio::runtime::Handle::current();

    let input = black_box(indoc::indoc! {"
        retry: 444

        : begin by specifying retry duration

        data: msg1 simple

        data: msg2
        data: with more on a newline

        data:msg3 without optional leading space

        data: msg4 with an ID
        id: 42

        retry: 999
        data: msg5 specifies new retry
        id: 43a

        event: msg
        data: msg6 is named

    "});

    b.bench(|| {
        let body_stream = stream::iter(input.as_bytes().chunks(7))
            .map(|line| Ok::<_, io::Error>(Bytes::from(line)))
            .interleave_pending();
        let body_reader = StreamReader::new(body_stream);

        let event_stream = FramedRead::new(body_reader, russe::Decoder::default());
        let event_stream = pin!(event_stream);

        let count = rt.block_on(event_stream.count());
        assert_eq!(count, 8);
    });
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    divan::main();
}
