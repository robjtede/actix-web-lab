//! Utilities for `ureq` v3.
//!
//! Enable the `ureq-3` feature and import [`UreqExt`] to read SSE responses with
//! a blocking iterator. See [`UreqExt::sse_iter`] for a usage example.
//!
//! Enable TLS features on your `ureq` dependency if you need HTTPS.

use std::io::{self, Read as _};

use bytes::BytesMut;
use tokio_util::codec::Decoder as _;
use ureq_3::{Body, BodyReader, http::Response};

use crate::{Decoder, Error, Event};

mod sealed {
    use super::*;

    pub trait Sealed {}
    impl Sealed for Response<Body> {}
}

/// SSE extension methods for `ureq` v3.
pub trait UreqExt: sealed::Sealed {
    /// Returns a blocking iterator of server-sent events.
    ///
    /// Each call to `next()` can block until an event, an error, or the end of the
    /// response body. The iterator ends after an I/O or decoding error.
    /// No async runtime is required.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use russe::ureq_3::UreqExt as _;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let response = ureq_3::get("http://localhost:8080/events").call()?;
    ///
    /// for event in response.sse_iter() {
    ///     println!("{:?}", event?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn sse_iter(self) -> impl Iterator<Item = Result<Event, Error>>;
}

impl UreqExt for Response<Body> {
    fn sse_iter(self) -> impl Iterator<Item = Result<Event, Error>> {
        SseIter {
            reader: self.into_body().into_reader(),
            decoder: Decoder::default(),
            buf: BytesMut::new(),
            eof: false,
            done: false,
        }
    }
}

struct SseIter {
    reader: BodyReader<'static>,
    decoder: Decoder,
    buf: BytesMut,
    eof: bool,
    done: bool,
}

impl Iterator for SseIter {
    type Item = Result<Event, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            let event = if self.eof {
                self.decoder.decode_eof(&mut self.buf)
            } else {
                self.decoder.decode(&mut self.buf)
            };

            match event {
                Ok(Some(event)) => return Some(Ok(event)),

                Ok(None) if self.eof => {
                    self.done = true;

                    return None;
                }

                Err(err) => {
                    self.done = true;

                    return Some(Err(err));
                }

                Ok(None) => {}
            }

            let mut chunk = [0; 8_192];

            match self.reader.read(&mut chunk) {
                Ok(0) => self.eof = true,

                Ok(len) => self.buf.extend_from_slice(&chunk[..len]),

                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}

                Err(err) => {
                    self.done = true;

                    return Some(Err(err.into()));
                }
            }
        }
    }
}
