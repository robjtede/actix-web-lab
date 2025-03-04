use std::{
    io::{self, BufReader},
    str,
    time::Duration,
};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use bytes::BytesMut;
use bytestring::ByteString;
use memchr::memmem;

use crate::{event::Event, message::Message, unix_lines::UnixLines, Error, NEWLINE, SSE_DELIMITER};

/// SSE decoder.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Decoder {
    event_finder: memmem::Finder<'static>,
    directive_finder: AhoCorasick,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            event_finder: memmem::Finder::new(SSE_DELIMITER),
            directive_finder: AhoCorasickBuilder::new()
                .match_kind(aho_corasick::MatchKind::LeftmostFirst)
                .build(
                    // patterns arranged in most-to-least common then with
                    // spaced variants first to support leftmost-first search
                    [
                        "data: ", "data:", // 0-1
                        "id: ", "id:", // 2-3
                        "event: ", "event:", // 4-5
                        "retry: ", "retry:", // 6-7
                        ": ", ":", // 8-9
                    ],
                )
                .unwrap(),
        }
    }
}

impl tokio_util::codec::Decoder for Decoder {
    type Item = Event;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // find the event delimiter \n\n or return None (more src data needed)
        let Some(idx_end_of_event) = self.event_finder.find(src) else {
            tracing::trace!("not enough data in buffer {src:?}");
            return Ok(None);
        };

        // full message received; remove from src buffer
        let buf = src.split_to(idx_end_of_event);

        // remove the delimiter from the buffer too
        drop(src.split_to(SSE_DELIMITER.len()));

        let lines_reader = UnixLines {
            rdr: BufReader::new(&*buf),
        };

        let mut message = Message {
            retry: None,
            event: None,
            data: ByteString::new(),
            id: None,
        };

        // TODO: if optimistic buffering is desired then remove this
        let mut data_buf = BytesMut::with_capacity(64);
        let mut message_event = false;

        for line in lines_reader {
            let mut line = line?;

            let matched = self.directive_finder.find(&line).expect("invalid line");

            debug_assert!(
                matched.start() == 0,
                "directive matched was not at beginning of line",
            );

            // discard matched directive bytes
            let _ = line.split_to(matched.end());
            let input = line;

            match matched.pattern().as_u64() {
                // data
                0 | 1 => {
                    if data_buf.is_empty() {
                        // first line
                        data_buf = input.into()
                    } else {
                        // additional lines
                        data_buf.extend_from_slice(&[NEWLINE]);
                        data_buf.extend_from_slice(&input);
                    }

                    message_event = true;
                }

                // id
                2 | 3 => {
                    let id = str::from_utf8(&input).unwrap();

                    message.id = Some(id.to_owned());
                    message_event = true;
                }

                // event
                4 | 5 => {
                    let event = ByteString::try_from(input).map_err(invalid_utf8)?;

                    message.event = Some(event);
                    message_event = true;
                }

                // retry
                6 | 7 => {
                    let input = str::from_utf8(&input).map_err(invalid_utf8)?;

                    message.retry = Some(Duration::from_millis(
                        input
                            .parse::<u64>()
                            .expect("retry should be an integer number of milliseconds"),
                    ))
                }

                // comment
                8 | 9 => {
                    let comment = ByteString::try_from(input).map_err(invalid_utf8)?;

                    return Ok(Some(Event::Comment(comment)));
                }

                _ => unreachable!("all search patterns are covered"),
            }
        }

        match message.retry {
            Some(retry) if !message_event => return Ok(Some(Event::Retry(retry))),
            _ => {}
        }

        if !data_buf.is_empty() {
            let data = ByteString::try_from(data_buf).map_err(invalid_utf8)?;

            message.data = data;
        }

        Ok(Some(Event::Message(message)))
    }
}

fn invalid_utf8(err: str::Utf8Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

#[cfg(test)]
mod tests {
    use std::{io, pin::pin};

    use bytes::Bytes;
    use futures_test::stream::StreamTestExt as _;
    use futures_util::{stream, StreamExt as _};
    use tokio_util::{codec::FramedRead, io::StreamReader};

    use super::*;
    use crate::assert_none;

    #[tokio::test]
    async fn reads_sse_frames() {
        let input = indoc::indoc! {"
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

        "};

        assert!(input.as_bytes().ends_with(SSE_DELIMITER));

        let body_stream = stream::iter(input.as_bytes().chunks(7))
            .map(|line| Ok::<_, io::Error>(Bytes::from(line)))
            .interleave_pending();
        let body_reader = StreamReader::new(body_stream);

        let event_stream = FramedRead::new(body_reader, Decoder::default());
        let mut event_stream = pin!(event_stream);

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(Event::Retry(Duration::from_millis(444)), ev);

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Comment("begin by specifying retry duration".into()),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(Event::Message(Message::data("msg1 simple")), ev);

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message::data("msg2\nwith more on a newline")),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message::data("msg3 without optional leading space")),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message {
                data: "msg4 with an ID".into(),
                id: Some("42".to_owned()),
                ..Default::default()
            }),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message {
                data: "msg5 specifies new retry".into(),
                id: Some("43a".to_owned()),
                retry: Some(Duration::from_millis(999)),
                event: None,
            }),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message {
                data: "msg6 is named".into(),
                event: Some("msg".into()),
                ..Default::default()
            }),
            ev,
        );

        // no more events in the stream
        assert_none!(event_stream.next().await);
    }

    #[tokio::test]
    async fn errors_on_invalid_utf8() {
        let input = b"data: invalid\xC3\x28msg\n\n".as_slice();

        assert!(input.ends_with(SSE_DELIMITER));

        let body_stream =
            stream::once(async { input }).map(|line| Ok::<_, io::Error>(Bytes::from(line)));
        let body_reader = StreamReader::new(body_stream);

        let event_stream = FramedRead::new(body_reader, Decoder::default());
        let mut event_stream = pin!(event_stream);

        let err = event_stream.next().await.unwrap().unwrap_err();
        assert_eq!(err.to_string(), "I/O error");

        // no more events in the stream
        assert_none!(event_stream.next().await);
    }
}
