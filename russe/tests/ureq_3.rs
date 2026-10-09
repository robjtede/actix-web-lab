//! Tests for the Ureq 3 integration.

#![cfg(feature = "ureq-3")]

use std::{
    collections::VecDeque,
    io::{self, BufRead as _, BufReader, Read, Write as _},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

use russe::{Error, Event, Message, ureq_3::UreqExt as _};
use ureq_3::{Agent, Body, http::Response};

struct ScriptedReader(VecDeque<io::Result<&'static [u8]>>);

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.pop_front() {
            Some(Ok(chunk)) => {
                assert!(chunk.len() <= buf.len());

                buf[..chunk.len()].copy_from_slice(chunk);

                Ok(chunk.len())
            }

            Some(Err(err)) => Err(err),

            None => Ok(0),
        }
    }
}

#[test]
fn response_iterator_decodes_sse_events() {
    let body = Body::builder()
        .data("id: 42\nevent: update\ndata: hello\n\n: heartbeat\n\nretry: 1000\n\n");
    let events = Response::new(body)
        .sse_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        events,
        vec![
            Event::Message(Message {
                data: "hello".into(),
                event: Some("update".into()),
                retry: None,
                id: Some("42".into()),
            }),
            Event::Comment("heartbeat".into()),
            Event::Retry(Duration::from_secs(1)),
        ],
    );
}

#[test]
fn decodes_fragmented_utf8_and_crlf_delimiters() {
    let chunks: &[&[u8]] = &[
        b"unknown: ignored\n\n: heart",
        b"beat\r\ndata: caf\xc3",
        b"\xa9\r\n\r",
        b"\n",
    ];
    let body = Body::builder().reader(ScriptedReader(
        chunks.iter().map(|&chunk| Ok(chunk)).collect(),
    ));
    let events = Response::new(body)
        .sse_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        events,
        vec![
            Event::Comment("heartbeat".into()),
            Event::Message(Message {
                data: "café".into(),
                event: None,
                retry: None,
                id: None,
            }),
        ],
    );
}

#[test]
fn yields_buffered_events_before_reading_again() {
    let body = Body::builder().reader(ScriptedReader(VecDeque::from([
        Ok(b": heartbeat\ndata: hello\n\n".as_slice()),
        Err(io::Error::other("body read failed")),
    ])));
    let mut events = Response::new(body).sse_iter();

    assert_eq!(
        events.next().unwrap().unwrap(),
        Event::Comment("heartbeat".into()),
    );
    assert_eq!(
        events.next().unwrap().unwrap(),
        Event::Message(Message {
            data: "hello".into(),
            event: None,
            retry: None,
            id: None,
        }),
    );

    let err = events.next().unwrap().unwrap_err();

    assert!(matches!(err, Error::Io { .. }), "got: {err:?}");
    assert!(events.next().is_none());
    assert!(events.next().is_none());
}

#[test]
fn retries_interrupted_reads() {
    let body = Body::builder().reader(ScriptedReader(VecDeque::from([
        Err(io::Error::from(io::ErrorKind::Interrupted)),
        Ok(b"data: hello\n\n".as_slice()),
    ])));
    let events = Response::new(body)
        .sse_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(
        events,
        vec![Event::Message(Message {
            data: "hello".into(),
            event: None,
            retry: None,
            id: None,
        })],
    );
}

#[test]
fn returns_invalid_utf8_error_once() {
    let body = Body::builder().data(b"data: \xff\n\ndata: ignored\n\n".as_slice());
    let mut events = Response::new(body).sse_iter();
    let err = events.next().unwrap().unwrap_err();

    assert!(matches!(err, Error::InvalidUtf8 { .. }), "got: {err:?}");
    assert!(events.next().is_none());
    assert!(events.next().is_none());
}

#[test]
fn returns_incomplete_frame_error_once() {
    let body = Body::builder().data("data: incomplete\n");
    let mut events = Response::new(body).sse_iter();
    let err = events.next().unwrap().unwrap_err();

    assert!(matches!(err, Error::Io { .. }), "got: {err:?}");
    assert!(events.next().is_none());
    assert!(events.next().is_none());
}

#[test]
fn empty_body_ends_iteration() {
    let body = Body::builder().data("");
    let mut events = Response::new(body).sse_iter();

    assert!(events.next().is_none());
    assert!(events.next().is_none());
}

#[test]
fn yields_event_before_http_response_ends() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://{}/events", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    let server = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();

        loop {
            line.clear();
            assert_ne!(socket.read_line(&mut line).unwrap(), 0);

            if line == "\r\n" {
                break;
            }
        }

        socket
            .get_mut()
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: hello\n\n",
            )
            .unwrap();

        // Keep the response open until the client receives the event.
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });

    let agent: Agent = Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let mut events = agent.get(&url).call().unwrap().sse_iter();
    let event = events.next().unwrap().unwrap();

    tx.send(()).unwrap();
    server.join().unwrap();

    assert_eq!(
        event,
        Event::Message(Message {
            data: "hello".into(),
            event: None,
            retry: None,
            id: None,
        }),
    );
    assert!(events.next().is_none());
}
