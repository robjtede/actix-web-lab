//! Tests for initial requests in both Reqwest integrations.

#![cfg(any(feature = "reqwest-0_12", feature = "reqwest-0_13"))]

macro_rules! manager_tests {
    ($reqwest:ident) => {
        mod $reqwest {
            use std::{error::Error as _, time::Duration};

            use ::$reqwest::Client;
            use russe::{Error, Event, $reqwest::Manager};
            use tokio::{
                io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
                net::TcpListener,
                task::JoinHandle,
            };

            async fn http_server(responses: &'static [&'static [u8]]) -> (String, JoinHandle<()>) {
                let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
                let url = format!("http://{}/events", listener.local_addr().unwrap());

                let task = tokio::spawn(async move {
                    for response in responses {
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

                        socket.get_mut().write_all(response).await.unwrap();
                    }
                });

                (url, task)
            }

            fn client() -> Client {
                Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap()
            }

            #[tokio::test]
            async fn returns_initial_request_errors() {
                let (url, server) = http_server(&[b""]).await;
                let client = client();
                let req = client.get(url).build().unwrap();
                let mut manager = Manager::new(&client, req);

                let err = manager
                    .send()
                    .await
                    .expect_err("The initial request failure must return from send()");

                assert!(matches!(err, Error::Http { .. }), "got: {err:?}");
                assert!(err.source().unwrap().is::<::$reqwest::Error>());

                server.await.unwrap();
            }

            #[tokio::test]
            async fn retries_after_initial_request_error() {
                let (url, server) = http_server(&[
                    b"",
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 8\r\nConnection: close\r\n\r\n: ping\n\n",
                ])
                .await;
                let client = client();
                let req = client.get(url).build().unwrap();
                let mut manager = Manager::new(&client, req);

                manager.send().await.unwrap_err();

                let (task, mut events) = manager.send().await.unwrap();
                let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();

                assert_eq!(event, Event::Comment("ping".into()));

                task.await.unwrap();
                server.await.unwrap();
            }

            #[tokio::test]
            async fn sends_body_errors_through_event_receiver() {
                let (url, server) = http_server(&[
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1\r\nConnection: close\r\n\r\n",
                ])
                .await;
                let client = client();
                let req = client.get(url).build().unwrap();
                let mut manager = Manager::new(&client, req);

                let (task, mut events) = manager.send().await.unwrap();
                let err = tokio::time::timeout(Duration::from_secs(5), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap_err();

                assert!(matches!(err, Error::Io { .. }), "got: {err:?}");

                task.await.unwrap();
                server.await.unwrap();
            }
        }
    };
}

#[cfg(feature = "reqwest-0_12")]
manager_tests!(reqwest_0_12);

#[cfg(feature = "reqwest-0_13")]
manager_tests!(reqwest_0_13);
