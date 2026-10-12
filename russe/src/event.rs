use std::{fmt::Write, time::Duration};

use bytes::BytesMut;
use bytestring::ByteString;

use crate::{Error, Message};

/// An SSE event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Message event.
    Message(Message),

    /// Comment event.
    Comment(ByteString),

    /// Retry recommendation event.
    Retry(Duration),
}

impl Event {
    /// Encodes the event into a UTF-8 string, including the final blank line.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be encoded. See [`crate::Encoder`] for field requirements.
    ///
    /// # Examples
    ///
    /// ```
    /// use russe::Event;
    ///
    /// let encoded = Event::Comment("keep-alive".into()).into_bytestring()?;
    /// assert_eq!(encoded, ": keep-alive\n\n");
    /// # Ok::<_, russe::Error>(())
    /// ```
    pub fn into_bytestring(self) -> crate::Result<ByteString> {
        let mut dst = String::new();

        self.encode_into(&mut dst)?;

        Ok(dst.into())
    }

    pub(crate) fn encode_into(self, dst: &mut impl EncodeBuffer) -> crate::Result<()> {
        match self {
            Event::Message(message) => {
                if let Some(id) = message.id.as_deref()
                    && let Some(offset) = id.find(['\0', '\r', '\n'])
                {
                    return Err(Error::invalid_field_value("id", id, offset));
                }

                if let Some(event) = message.event.as_deref()
                    && let Some(offset) = event.find(['\r', '\n'])
                {
                    return Err(Error::invalid_field_value("event", event, offset));
                }

                // Reserve known lengths without a second scan of the data.
                let capacity = message.data.len()
                    + 8
                    + message.id.as_ref().map_or(0, |id| id.len() + 5)
                    + message.event.as_ref().map_or(0, |event| event.len() + 8)
                    + message.retry.map_or(0, |_| RETRY_CAPACITY);

                dst.reserve(capacity);

                if let Some(id) = message.id {
                    let _ = dst.write_str("id: ");
                    let _ = dst.write_str(&id);
                    let _ = dst.write_char('\n');
                }

                if let Some(event) = message.event {
                    let _ = dst.write_str("event: ");
                    let _ = dst.write_str(&event);
                    let _ = dst.write_char('\n');
                }

                if let Some(retry) = message.retry {
                    encode_retry(dst, retry);
                }

                encode_lines(dst, "data: ", &message.data);
                let _ = dst.write_char('\n');
            }

            Event::Comment(comment) => {
                dst.reserve(comment.len() + 4);

                encode_lines(dst, ": ", &comment);
                let _ = dst.write_char('\n');
            }

            Event::Retry(retry) => {
                dst.reserve(RETRY_CAPACITY + 1);

                encode_retry(dst, retry);
                let _ = dst.write_char('\n');
            }
        }

        Ok(())
    }
}

// "retry: ", up to 23 decimal digits for Duration::MAX, and LF.
const RETRY_CAPACITY: usize = 7 + 23 + 1;

pub(crate) trait EncodeBuffer: Write {
    fn reserve(&mut self, additional: usize);
}

impl EncodeBuffer for String {
    fn reserve(&mut self, additional: usize) {
        String::reserve(self, additional);
    }
}

impl EncodeBuffer for BytesMut {
    fn reserve(&mut self, additional: usize) {
        BytesMut::reserve(self, additional);
    }
}

fn encode_retry(dst: &mut impl Write, retry: Duration) {
    // Writing to String or BytesMut cannot fail.
    let _ = writeln!(dst, "retry: {}", retry.as_millis());
}

/// Prefixes each line, preserves empty lines, and converts CRLF and CR to LF.
fn encode_lines(dst: &mut impl Write, prefix: &str, mut text: &str) {
    loop {
        let _ = dst.write_str(prefix);

        let Some(idx) = memchr::memchr2(b'\r', b'\n', text.as_bytes()) else {
            let _ = dst.write_str(text);
            let _ = dst.write_char('\n');

            break;
        };

        let _ = dst.write_str(&text[..idx]);
        let _ = dst.write_char('\n');

        let line_ending = text.as_bytes()[idx];
        text = &text[idx + 1..];

        if line_ending == b'\r' {
            text = text.strip_prefix('\n').unwrap_or(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_events_into_bytestring() {
        let cases = [
            (
                Event::Message(Message {
                    data: "café\r\n世界".into(),
                    event: Some("update".into()),
                    id: Some("42".into()),
                    retry: None,
                }),
                "id: 42\nevent: update\ndata: café\ndata: 世界\n\n",
            ),
            (Event::Comment("keep-alive".into()), ": keep-alive\n\n"),
            (Event::Retry(Duration::from_millis(1500)), "retry: 1500\n\n"),
        ];

        for (event, expected) in cases {
            assert_eq!(event.into_bytestring().unwrap(), expected);
        }
    }

    #[test]
    fn preserves_empty_lines_and_utf8_with_each_line_ending() {
        for (text, expected) in [
            ("", "data: \n\n"),
            ("\r\n", "data: \ndata: \n\n"),
            ("\r\r\n\n", "data: \ndata: \ndata: \ndata: \n\n"),
            (
                "é\r\n世界\r🦀\n",
                "data: é\ndata: 世界\ndata: 🦀\ndata: \n\n",
            ),
            ("é\0世界", "data: é\0世界\n\n"),
        ] {
            let event = Event::Message(Message::data(text));

            assert_eq!(
                event.into_bytestring().unwrap(),
                expected,
                "input: {text:?}"
            );

            let event = Event::Comment(text.into());

            assert_eq!(
                event.into_bytestring().unwrap(),
                expected.replace("data: ", ": "),
                "input: {text:?}",
            );
        }
    }

    #[test]
    fn encodes_maximum_retry_in_messages_and_retry_events() {
        for event in [
            Event::Retry(Duration::MAX),
            Event::Message(Message {
                retry: Some(Duration::MAX),
                ..Message::data("")
            }),
        ] {
            let expected = match event {
                Event::Retry(_) => "retry: 18446744073709551615999\n\n",
                _ => "retry: 18446744073709551615999\ndata: \n\n",
            };

            assert_eq!(event.into_bytestring().unwrap(), expected);
        }
    }

    #[test]
    fn rejects_invalid_message_fields() {
        for (id, event) in [(Some("bad\0id"), None), (None, Some("bad\nevent"))] {
            let message = Message {
                data: "hello".into(),
                event: event.map(Into::into),
                id: id.map(Into::into),
                retry: None,
            };

            assert!(matches!(
                Event::Message(message).into_bytestring(),
                Err(crate::Error::InvalidFieldValue { .. })
            ));
        }
    }

    #[test]
    fn invalid_field_errors_report_details() {
        for (id, event, expected) in [
            (Some("bad\0id"), None, "Invalid SSE id: '\\0' at byte 3"),
            (
                None,
                Some("bad\nevent"),
                "Invalid SSE event: '\\n' at byte 3",
            ),
            (Some("é\rid"), None, "Invalid SSE id: '\\r' at byte 2"),
            (
                Some("bad\r\nid"),
                Some("bad\nevent"),
                "Invalid SSE id: '\\r' at byte 3",
            ),
            (
                None,
                Some("\"bad\\name\n"),
                "Invalid SSE event: '\\n' at byte 9",
            ),
        ] {
            let message = Message {
                data: "hello".into(),
                event: event.map(Into::into),
                id: id.map(Into::into),
                retry: None,
            };

            let err = Event::Message(message).into_bytestring().unwrap_err();

            assert_eq!(err.to_string(), expected);
        }
    }

    #[test]
    fn invalid_field_errors_report_byte_offsets() {
        for (id, expected_offset) in [
            (format!("{}\n{}", "a".repeat(100), "z".repeat(100)), 100),
            (format!("{}\n{}", "é".repeat(100), "世".repeat(100)), 200),
            (
                format!("{}\n{}", "\\".repeat(100), "\u{1}".repeat(100)),
                100,
            ),
            (format!("\n{}", "世".repeat(100)), 0),
            (format!("{}\n", "é".repeat(100)), 200),
        ] {
            let message = Message {
                data: "hello".into(),
                event: None,
                id: Some(id.into()),
                retry: None,
            };

            let err = Event::Message(message).into_bytestring().unwrap_err();
            let Error::InvalidFieldValue {
                field,
                character,
                offset,
            } = &err
            else {
                panic!("Unexpected error: {err:?}");
            };

            assert_eq!((*field, *character, *offset), ("id", '\n', expected_offset));
        }
    }
}
