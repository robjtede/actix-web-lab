use std::time::Duration;

use bytes::BytesMut;
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
    /// Encodes the event into a UTF-8 string, including the final blank line.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be encoded. See [`Encoder`] for field requirements.
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
        let mut buf = BytesMut::new();

        Encoder::default().encode(self, &mut buf)?;

        ByteString::try_from(buf.freeze()).map_err(|_| crate::Error::Invalid)
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
                Err(crate::Error::Invalid)
            ));
        }
    }
}
