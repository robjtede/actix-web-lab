//! Tests for the public SSE encoder.

use std::time::Duration;

use bytes::BytesMut;
use futures_util::SinkExt as _;
use russe::{Encoder, Error, Event, Message};
use tokio_util::codec::{Decoder as _, Encoder as _, FramedWrite};

#[test]
fn encodes_message_data() {
    let message = Message {
        data: "héllo".into(),
        event: None,
        retry: None,
        id: None,
    };
    let mut output = BytesMut::new();

    Encoder::default()
        .encode(Event::Message(message), &mut output)
        .unwrap();

    assert_eq!(output, "data: héllo\n\n");
}

#[test]
fn encodes_empty_and_multiline_data() {
    for (data, expected) in [
        ("", "data: \n\n"),
        (
            "\nfirst\n\n last\n",
            "data: \ndata: first\ndata: \ndata:  last\ndata: \n\n",
        ),
        (
            "first\r\nsecond\rthird\n",
            "data: first\ndata: second\ndata: third\ndata: \n\n",
        ),
    ] {
        let message = Message {
            data: data.into(),
            event: None,
            retry: None,
            id: None,
        };
        let mut output = BytesMut::new();

        Encoder::default()
            .encode(Event::Message(message), &mut output)
            .unwrap();

        assert_eq!(output, expected, "input: {data:?}");
    }
}

#[test]
fn encodes_message_fields() {
    let message = Message {
        data: "payload".into(),
        event: Some(" update".into()),
        retry: Some(Duration::from_micros(1_234_567)),
        id: Some("42".into()),
    };
    let mut output = BytesMut::new();

    Encoder::default()
        .encode(Event::Message(message), &mut output)
        .unwrap();

    assert_eq!(
        output,
        "id: 42\nevent:  update\nretry: 1234\ndata: payload\n\n",
    );
}

#[test]
fn encodes_comments() {
    for (comment, expected) in [
        ("", ": \n\n"),
        ("keep-alive", ": keep-alive\n\n"),
        (
            "\nfirst\r\nsecond\r\n\rthird\n",
            ": \n: first\n: second\n: \n: third\n: \n\n",
        ),
    ] {
        let mut output = BytesMut::new();

        Encoder::default()
            .encode(Event::Comment(comment.into()), &mut output)
            .unwrap();

        assert_eq!(output, expected, "input: {comment:?}");
    }
}

#[test]
fn encodes_retry_delays_in_milliseconds() {
    for (retry, expected) in [
        (Duration::ZERO, "retry: 0\n\n"),
        (Duration::from_secs(10), "retry: 10000\n\n"),
        (Duration::from_micros(1_999), "retry: 1\n\n"),
        (Duration::MAX, "retry: 18446744073709551615999\n\n"),
    ] {
        let mut output = BytesMut::new();

        Encoder::default()
            .encode(Event::Retry(retry), &mut output)
            .unwrap();

        assert_eq!(output, expected);
    }
}

#[test]
fn rejects_invalid_message_fields_without_changing_output() {
    for (id, event) in [
        ("bad\nid", "update"),
        ("bad\rid", "update"),
        ("bad\r\nid", "update"),
        ("bad\0id", "update"),
        ("42", "bad\nevent"),
        ("42", "bad\revent"),
        ("42", "bad\r\nevent"),
    ] {
        let message = Message {
            data: "payload".into(),
            event: Some(event.into()),
            retry: Some(Duration::from_secs(1)),
            id: Some(id.into()),
        };
        let mut output = BytesMut::from("data: previous\n\n");

        let result = Encoder::default().encode(Event::Message(message), &mut output);

        assert!(matches!(result, Err(Error::Invalid)), "got: {result:?}");
        assert_eq!(output, "data: previous\n\n");
    }
}

#[test]
fn encodes_empty_message_fields() {
    let message = Message {
        data: "".into(),
        event: Some("".into()),
        retry: Some(Duration::ZERO),
        id: Some("".into()),
    };
    let mut output = BytesMut::new();

    Encoder::default()
        .encode(Event::Message(message), &mut output)
        .unwrap();

    assert_eq!(output, "id: \nevent: \nretry: 0\ndata: \n\n");
}

#[tokio::test]
async fn writes_events_with_framed_write() {
    let mut framed = FramedWrite::new(Vec::new(), Encoder::default());

    framed
        .feed(Event::Retry(Duration::from_secs(1)))
        .await
        .unwrap();
    framed
        .feed(Event::Comment("keep-alive".into()))
        .await
        .unwrap();
    framed
        .feed(Event::Message(Message {
            data: "first\nsecond\n".into(),
            event: Some("update".into()),
            retry: None,
            id: Some("42".into()),
        }))
        .await
        .unwrap();
    framed.flush().await.unwrap();

    assert_eq!(
        framed.into_inner(),
        b"retry: 1000\n\n: keep-alive\n\nid: 42\nevent: update\ndata: first\ndata: second\ndata: \n\n",
    );
}

#[test]
fn encoded_events_can_be_decoded() {
    let events = [
        Event::Retry(Duration::from_secs(1)),
        Event::Comment("keep-alive".into()),
        Event::Message(Message {
            data: "first\nsecond\n".into(),
            event: Some("update".into()),
            retry: Some(Duration::from_millis(42)),
            id: Some("123".into()),
        }),
    ];
    let mut output = BytesMut::new();
    let mut encoder = Encoder::default();
    let mut decoder = russe::Decoder::default();

    for event in &events {
        encoder.encode(event.clone(), &mut output).unwrap();
    }

    for event in events {
        assert_eq!(decoder.decode(&mut output).unwrap(), Some(event));
    }

    assert!(output.is_empty());
}
