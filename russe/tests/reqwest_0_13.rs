//! Tests for the Reqwest 0.13 integration.

#![cfg(feature = "reqwest-0_13")]

use std::time::Duration;

use futures_util::TryStreamExt as _;
use reqwest_0_13::Client;
use russe::{
    Event, Message,
    reqwest_0_13::{Manager, ReqwestExt as _},
};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    net::TcpListener,
    task::JoinHandle,
};
use tokio_stream::wrappers::UnboundedReceiverStream;

async fn sse_server() -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();

    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();

        loop {
            line.clear();
            assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);

            if line == "\r\n" {
                break;
            }
        }

        let body = "id: 42\nevent: update\ndata: hello\n\n: heartbeat\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        );

        socket
            .get_mut()
            .write_all(response.as_bytes())
            .await
            .unwrap();
    });

    (format!("http://{addr}/events"), task)
}

#[tokio::test]
async fn response_stream_decodes_sse_events() {
    let (url, server) = sse_server().await;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let events = client
        .get(url)
        .send()
        .await
        .unwrap()
        .sse_stream()
        .try_collect::<Vec<_>>()
        .await
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
        ],
    );

    server.await.unwrap();
}

#[tokio::test]
async fn manager_delivers_sse_events() {
    let (url, server) = sse_server().await;
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let req = client.get(url).build().unwrap();
    let mut manager = Manager::new(&client, req);

    let (task, events) = manager.send().await.unwrap();
    drop(manager);

    let events = tokio::time::timeout(
        Duration::from_secs(5),
        UnboundedReceiverStream::new(events).try_collect::<Vec<_>>(),
    )
    .await
    .unwrap()
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
        ],
    );

    task.await.unwrap();
    server.await.unwrap();
}
