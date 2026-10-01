use std::{io::Write as _, time::Duration};

use bytes::{BufMut as _, BytesMut};

use crate::{Error, Event};

/// SSE encoder for use with [`tokio_util::codec::FramedWrite`].
///
/// Implements [`tokio_util::codec::Encoder<Event>`]. Each event ends with a blank line.
/// CRLF and bare CR in message data and comments are converted to LF. Empty lines are preserved.
/// Retry delays are written in whole milliseconds. Fractions of a millisecond are discarded.
///
/// Returns [`Error::Invalid`] if an event name contains CR or LF, or an ID contains NUL, CR, or LF.
/// These fields are checked before any bytes are added to the output buffer.
///
/// # Examples
///
/// ```
/// use bytes::BytesMut;
/// use russe::{Encoder, Event};
/// use tokio_util::codec::Encoder as _;
///
/// let mut output = BytesMut::new();
/// Encoder::default().encode(Event::Comment("keep-alive".into()), &mut output)?;
/// assert_eq!(output, ": keep-alive\n\n");
/// # Ok::<_, russe::Error>(())
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Encoder {}

impl tokio_util::codec::Encoder<Event> for Encoder {
    type Error = Error;

    fn encode(&mut self, item: Event, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            Event::Message(message) => {
                if message
                    .id
                    .as_deref()
                    .is_some_and(|id| id.contains(['\0', '\r', '\n']))
                    || message
                        .event
                        .as_deref()
                        .is_some_and(|event| event.contains(['\r', '\n']))
                {
                    return Err(Error::Invalid);
                }

                if let Some(id) = message.id {
                    dst.extend_from_slice(b"id: ");
                    dst.extend_from_slice(id.as_bytes());
                    dst.extend_from_slice(b"\n");
                }

                if let Some(event) = message.event {
                    dst.extend_from_slice(b"event: ");
                    dst.extend_from_slice(event.as_bytes());
                    dst.extend_from_slice(b"\n");
                }

                if let Some(retry) = message.retry {
                    encode_retry(dst, retry)?;
                }

                encode_lines(dst, b"data: ", &message.data);
                dst.extend_from_slice(b"\n");

                Ok(())
            }

            Event::Comment(comment) => {
                encode_lines(dst, b": ", &comment);
                dst.extend_from_slice(b"\n");

                Ok(())
            }

            Event::Retry(retry) => {
                encode_retry(dst, retry)?;
                dst.extend_from_slice(b"\n");

                Ok(())
            }
        }
    }
}

fn encode_retry(dst: &mut BytesMut, retry: Duration) -> Result<(), Error> {
    writeln!(dst.writer(), "retry: {}", retry.as_millis())?;

    Ok(())
}

fn encode_lines(dst: &mut BytesMut, prefix: &[u8], mut text: &str) {
    loop {
        dst.extend_from_slice(prefix);

        let Some(idx) = text.find(['\r', '\n']) else {
            dst.extend_from_slice(text.as_bytes());
            dst.extend_from_slice(b"\n");

            break;
        };

        dst.extend_from_slice(&text.as_bytes()[..idx]);
        dst.extend_from_slice(b"\n");

        let line_ending = text.as_bytes()[idx];
        text = &text[idx + 1..];

        if line_ending == b'\r' {
            text = text.strip_prefix('\n').unwrap_or(text);
        }
    }
}
