use std::io;

use futures_util::{stream::BoxStream, TryStreamExt as _};
use tokio_util::{codec::FramedRead, io::StreamReader};

use crate::{Decoder, Error, Event};

mod sealed {
    pub trait Sealed {}
    impl Sealed for reqwest::Response {}
}

/// SSE extension methods for `reqwest` v0.12.
pub trait ReqwestExt: sealed::Sealed {
    /// Returns a stream of server-sent events.
    fn sse_stream(self) -> BoxStream<'static, Result<Event, Error>>;
}

impl ReqwestExt for reqwest::Response {
    fn sse_stream(self) -> BoxStream<'static, Result<Event, Error>> {
        let body_stream = self.bytes_stream().map_err(io::Error::other);
        let body_reader = StreamReader::new(body_stream);

        let frame_reader = FramedRead::new(body_reader, Decoder::default());

        Box::pin(frame_reader)
    }
}
