use std::time::Duration;

use bytes::{Bytes, BytesMut};
use bytestring::ByteString;
use tokio_util::codec::Encoder as _;

use crate::{Encoder, Message};

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
    /// Encodes the event into bytes, including the final blank line.
    ///
    /// Uses the same format and validation as [`Encoder`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Invalid`] if an event name contains CR or LF, or an ID contains
    /// NUL, CR, or LF.
    ///
    /// # Examples
    ///
    /// ```
    /// use russe::Event;
    ///
    /// let bytes = Event::Comment("keep-alive".into()).into_bytes()?;
    /// assert_eq!(bytes, ": keep-alive\n\n");
    /// # Ok::<_, russe::Error>(())
    /// ```
    pub fn into_bytes(self) -> crate::Result<Bytes> {
        let mut buf = BytesMut::new();

        Encoder::default().encode(self, &mut buf)?;

        Ok(buf.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_events_into_bytes() {
        let cases = [
            (
                Event::Message(Message {
                    data: "first\r\nsecond".into(),
                    event: Some("update".into()),
                    id: Some("42".into()),
                    retry: None,
                }),
                "id: 42\nevent: update\ndata: first\ndata: second\n\n",
            ),
            (Event::Comment("keep-alive".into()), ": keep-alive\n\n"),
            (Event::Retry(Duration::from_millis(1500)), "retry: 1500\n\n"),
        ];

        for (event, expected) in cases {
            assert_eq!(event.into_bytes().unwrap(), expected);
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
                Event::Message(message).into_bytes(),
                Err(crate::Error::Invalid)
            ));
        }
    }
}
