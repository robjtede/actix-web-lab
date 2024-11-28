use std::{
    io::{BufRead as _, BufReader},
    time::Duration,
};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use bytes::BytesMut;
use bytestring::ByteString;
use memchr::memmem;

use crate::{event::Event, message::Message, Error, NEWLINE, SSE_DELIMITER};

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

        // TODO: consider if using lines (which also does \r\n) is correct
        // TODO: replace with ByteString::read_until \n
        let lines_reader = BufReader::new(&*buf).lines();

        let mut message = Message {
            retry: None,
            event: None,
            data: ByteString::new(),
            id: None,
        };

        // TODO: if optimistic buffering is desired then remove this
        let mut data_buf = BytesMut::new();
        let mut message_event = false;

        for line in lines_reader {
            let line = line?;

            let matched = self.directive_finder.find(&line).expect("invalid line");

            if matched.start() != 0 {
                panic!("directive matched was not at beginning of line")
            }

            let (_directive, input) = line.split_at(matched.end());

            match matched.pattern().as_u64() {
                // data
                0 | 1 => {
                    if data_buf.is_empty() {
                        // first line
                        data_buf = input.as_bytes().into()
                    } else {
                        // additional lines
                        data_buf.extend_from_slice(&[NEWLINE]);
                        data_buf.extend_from_slice(input.as_bytes());
                    }

                    message_event = true;
                }

                // id
                2 | 3 => {
                    message.id = Some(input.to_owned());
                    message_event = true;
                }

                // event
                4 | 5 => {
                    message.event = Some(input.into());
                    message_event = true;
                }

                // retry
                6 | 7 => {
                    message.retry = Some(Duration::from_millis(
                        input
                            .parse::<u64>()
                            .expect("retry should be an integer number of milliseconds"),
                    ))
                }

                // comment
                8 | 9 => return Ok(Some(Event::Comment(input.into()))),

                _ => unreachable!("all search patterns are covered"),
            }
        }

        match message.retry {
            Some(retry) if !message_event => return Ok(Some(Event::Retry(retry))),
            _ => {}
        }

        if !data_buf.is_empty() {
            message.data = ByteString::try_from(data_buf).expect("Invalid UTF-8");
        }

        Ok(Some(Event::Message(message)))
    }
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
}
