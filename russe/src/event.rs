use std::{fmt::Write as _, time::Duration};

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

                if let Some(id) = message.id {
                    dst.push_str("id: ");
                    dst.push_str(&id);
                    dst.push('\n');
                }

                if let Some(event) = message.event {
                    dst.push_str("event: ");
                    dst.push_str(&event);
                    dst.push('\n');
                }

                if let Some(retry) = message.retry {
                    encode_retry(&mut dst, retry);
                }

                encode_lines(&mut dst, "data: ", &message.data);
                dst.push('\n');
            }

            Event::Comment(comment) => {
                encode_lines(&mut dst, ": ", &comment);
                dst.push('\n');
            }

            Event::Retry(retry) => {
                encode_retry(&mut dst, retry);
                dst.push('\n');
            }
        }

        Ok(dst.into())
    }
}

fn encode_retry(dst: &mut String, retry: Duration) {
    // Writing to a String cannot fail.
    let _ = writeln!(dst, "retry: {}", retry.as_millis());
}

/// Prefixes each line, preserves empty lines, and converts CRLF and CR to LF.
fn encode_lines(dst: &mut String, prefix: &str, mut text: &str) {
    loop {
        dst.push_str(prefix);

        let Some(idx) = text.find(['\r', '\n']) else {
            dst.push_str(text);
            dst.push('\n');

            break;
        };

        dst.push_str(&text[..idx]);
        dst.push('\n');

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
            (Some("bad\0id"), None, "invalid SSE id: '\\0' at byte 3"),
            (
                None,
                Some("bad\nevent"),
                "invalid SSE event: '\\n' at byte 3",
            ),
            (Some("é\rid"), None, "invalid SSE id: '\\r' at byte 2"),
            (
                Some("bad\r\nid"),
                Some("bad\nevent"),
                "invalid SSE id: '\\r' at byte 3",
            ),
            (
                None,
                Some("\"bad\\name\n"),
                "invalid SSE event: '\\n' at byte 9",
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
