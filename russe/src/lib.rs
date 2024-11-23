//! SSE

#![forbid(unsafe_code)]
#![allow(missing_docs, unused_imports)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use std::io::{self, BufRead as _, BufReader};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, PatternID};
use futures_util::Stream;
use memchr::{memchr, memmem};
use tokio_util::{bytes::BytesMut, codec::Decoder};

mod error;

pub use self::error::Error;

const NEWLINE: u8 = b'\n';
const SSE_DELIMITER: &[u8] = b"\n\n";

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// millis
    retry: Option<u64>,

    /// named event
    event: Option<String>,

    /// is always string ?
    data: Option<String>,

    /// is always numeric ?
    id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Retry(u64),
    Comment,
    Message(Message),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Codec {
    event_finder: memmem::Finder<'static>,
    directive_finder: AhoCorasick,
}

impl Default for Codec {
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

impl Decoder for Codec {
    type Item = Event;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // find the event delimiter \n\n or return None (more src data needed)
        let Some(idx_end_of_event) = self.event_finder.find(src) else {
            eprintln!("not enough data in buffer {src:?}");
            return Ok(None);
        };

        // full message received; remove from src buffer
        let buf = src.split_to(idx_end_of_event);
        eprintln!("{buf:?}");

        // remove the delimiter from the buffer too
        drop(src.split_to(SSE_DELIMITER.len()));

        // TODO: consider if using lines (which also does \r\n) is correct
        let lines_reader = BufReader::new(&*buf).lines();

        let mut message = Message {
            retry: None,
            event: None,
            data: None,
            id: None,
        };

        let mut message_event = false;

        for line in lines_reader {
            // TODO: replace with ?
            let line = line.unwrap();

            let matched = self.directive_finder.find(&line).expect("invalid line");

            if matched.start() != 0 {
                panic!("directive matched was not at beginning of line")
            }

            let input = line[matched.end()..].to_owned();

            match matched.pattern().as_u64() {
                // data
                0 | 1 => {
                    match &mut message.data {
                        Some(data) => {
                            data.push(NEWLINE as char);
                            data.push_str(&input);
                        }
                        None => message.data = Some(input),
                    }

                    message_event = true;
                }

                // id
                2 | 3 => {
                    message.id = Some(input.parse().expect("ID should be an integer"));
                    message_event = true;
                }

                // event
                4 | 5 => {
                    message.event = Some(input);
                    message_event = true;
                }

                // retry
                6 | 7 => {
                    message.retry = Some(
                        input
                            .parse()
                            .expect("retry should be an integer number of milliseconds"),
                    )
                }

                // comment
                8 | 9 => return Ok(Some(Event::Comment)),

                _ => unreachable!("all search patterns are covered"),
            }
        }

        match message.retry {
            Some(retry) if !message_event => return Ok(Some(Event::Retry(retry))),
            _ => {}
        }

        Ok(Some(Event::Message(message)))
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use futures_test::stream::StreamTestExt as _;
    use futures_util::{stream, StreamExt as _};
    use tokio_util::{bytes::Bytes, codec::FramedRead, io::StreamReader};

    use super::*;

    impl Message {
        fn data(data: impl Into<String>) -> Self {
            Self {
                data: Some(data.into()),
                ..Default::default()
            }
        }
    }

    // impl default to make tests simpler
    #[allow(clippy::derivable_impls)]
    impl Default for Message {
        fn default() -> Self {
            Self {
                retry: None,
                event: None,
                data: None,
                id: None,
            }
        }
    }

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
            id: 43

            event: msg
            data: msg6 is named

        "};

        assert!(input.as_bytes().ends_with(SSE_DELIMITER));

        let body_stream = stream::iter(input.as_bytes().chunks(7))
            .map(|line| Ok::<_, io::Error>(Bytes::from(line)))
            .interleave_pending();
        let body_reader = StreamReader::new(body_stream);

        let event_stream = FramedRead::new(body_reader, Codec::default());
        let mut event_stream = pin!(event_stream);

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(Event::Retry(444), ev);

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(Event::Comment, ev);

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
                data: Some("msg4 with an ID".to_owned()),
                id: Some(42),
                ..Default::default()
            }),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message {
                data: Some("msg5 specifies new retry".to_owned()),
                id: Some(43),
                retry: Some(999),
                event: None,
            }),
            ev,
        );

        let ev = event_stream.next().await.unwrap().unwrap();
        assert_eq!(
            Event::Message(Message {
                data: Some("msg6 is named".to_owned()),
                event: Some("msg".to_owned()),
                ..Default::default()
            }),
            ev,
        );

        // no more events in the stream
        assert_none!(event_stream.next().await);
    }

    #[macro_export]
    macro_rules! assert_none {
        ($exp:expr) => {{
            let exp = $exp;
            assert!(exp.is_none(), "Expected None; got: {exp:?}");
        }};
    }
}
