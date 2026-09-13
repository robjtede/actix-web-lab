//! Behavior tests over real Actix Web TLS connections.

use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::{Arc, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

use actix_web::{App, HttpServer, dev::ServerHandle, web};
use hot_reload_rustls::{BuildError, CredentialError, Event, Watcher};

#[derive(Debug)]
struct WorkerKeyProvider;

impl rustls::crypto::KeyProvider for WorkerKeyProvider {
    fn load_private_key(
        &self,
        key: rustls::pki_types::PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn rustls::sign::SigningKey>, rustls::Error> {
        assert_eq!(thread::current().name(), Some(env!("CARGO_PKG_NAME")));
        rustls::crypto::aws_lc_rs::default_provider()
            .key_provider
            .load_private_key(key)
    }
}

struct Pair {
    cert: Vec<u8>,
    key: Vec<u8>,
    der: Vec<u8>,
}

fn pairs() -> &'static [Pair; 2] {
    static PAIRS: OnceLock<[Pair; 2]> = OnceLock::new();

    PAIRS.get_or_init(|| {
        [1, 2].map(|serial| {
            let key = rcgen::KeyPair::generate().unwrap();
            let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
            params.serial_number = Some(serial.into());
            let cert = params.self_signed(&key).unwrap();

            Pair {
                cert: cert.pem().into_bytes(),
                key: key.serialize_pem().into_bytes(),
                der: cert.der().to_vec(),
            }
        })
    })
}

struct Server {
    addr: SocketAddr,
    tls13: bool,
    defaults: bool,
    handle: ServerHandle,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        actix_web::rt::System::new().block_on(self.handle.stop(false));
        self.thread.take().unwrap().join().unwrap();
    }
}

fn start(cert: &Path, key: &Path, scenario: &str) -> (Server, Watcher, mpsc::Receiver<Event>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();

    let tls13 = matches!(scenario, "tls13" | "default");
    let defaults = matches!(scenario, "default" | "default12");
    let observer = scenario == "observer";

    let (ready, started) = mpsc::channel();
    let cert = cert.to_owned();
    let key = key.to_owned();

    let thread = thread::spawn(move || {
        let factory = || App::new().route("/", web::get().to(|| async { "rotating" }));

        // Initialization occurs before the async runtime starts.
        let mut provider = rustls::crypto::aws_lc_rs::default_provider();
        provider.key_provider = &WorkerKeyProvider;
        let provider = Arc::new(provider);
        let builder = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[if tls13 {
                &rustls::version::TLS13
            } else {
                &rustls::version::TLS12
            }])
            .unwrap()
            .with_no_client_auth();
        let (config, mut watcher) = if defaults {
            hot_reload_rustls::Builder::new(cert, key)
                .crypto_provider(Arc::clone(&provider))
                .build()
                .unwrap()
        } else {
            hot_reload_rustls::Builder::new(cert, key)
                .crypto_provider(Arc::clone(&provider))
                .tls_config(builder)
                .configure(|config| {
                    assert_eq!(thread::current().name(), Some(env!("CARGO_PKG_NAME")));
                    config.alpn_protocols = vec![b"http/1.1".to_vec()];

                    Ok(())
                })
                .build()
                .unwrap()
        };

        let events = if observer {
            let (tx, rx) = mpsc::channel();
            watcher
                .spawn_observer(move |event| {
                    assert!(thread::current().name().unwrap().ends_with("-observer"));
                    let _ = tx.send(event);
                })
                .unwrap();

            rx
        } else {
            watcher.take_events().unwrap()
        };

        actix_web::rt::System::new().block_on(async move {
            let server = HttpServer::new(factory)
                .workers(1)
                .disable_signals()
                .listen_rustls_0_23(listener, config)
                .unwrap()
                .run();

            ready.send((server.handle(), watcher, events)).unwrap();
            server.await.unwrap();
        });
    });

    let (handle, watcher, events) = started.recv_timeout(Duration::from_secs(15)).unwrap();
    (
        Server {
            addr,
            tls13,
            defaults,
            handle,
            thread: Some(thread),
        },
        watcher,
        events,
    )
}

fn served(server: &Server, sni: bool) -> Vec<u8> {
    // A fresh client per probe prevents session resumption.
    let mut roots = rustls::RootCertStore::empty();
    for pair in pairs() {
        roots
            .add(rustls::pki_types::CertificateDer::from(pair.der.clone()))
            .unwrap();
    }

    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_protocol_versions(&[if server.tls13 {
        &rustls::version::TLS13
    } else {
        &rustls::version::TLS12
    }])
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    config.enable_sni = sni;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.resumption = rustls::client::Resumption::disabled();

    let mut connection =
        rustls::ClientConnection::new(Arc::new(config), "localhost".try_into().unwrap()).unwrap();
    let mut tcp = TcpStream::connect_timeout(&server.addr, Duration::from_secs(3)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
    while connection.is_handshaking() {
        connection.complete_io(&mut tcp).unwrap();
    }

    assert_eq!(
        connection.protocol_version(),
        Some(if server.tls13 {
            rustls::ProtocolVersion::TLSv1_3
        } else {
            rustls::ProtocolVersion::TLSv1_2
        })
    );
    if !server.defaults {
        assert_eq!(connection.alpn_protocol(), Some(&b"http/1.1"[..]));
    }
    let der = connection.peer_certificates().unwrap()[0].to_vec();
    let mut tls = rustls::StreamOwned::new(connection, tcp);

    tls.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();

    let mut response = Vec::new();
    // Some TLS backends close after the HTTP response without close_notify.
    let _ = tls.read_to_end(&mut response);

    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert!(response.ends_with(b"rotating"));

    der
}

fn wait_for(events: &mpsc::Receiver<Event>, success: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        match events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
        {
            Event::Reloaded if success => return,
            Event::ReloadFailed { .. } if !success => return,
            Event::WatchFailed(error) => panic!("watch failed: {error}"),
            _ => {}
        }
    }
}

fn exercise(scenario: &str) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("keys")).unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join(if scenario == "debounce" {
        "key.pem"
    } else {
        "keys/key.pem"
    });

    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[0].key).unwrap();

    let (server, watcher, events) = start(&cert, &key, scenario);
    assert_eq!(served(&server, true), pairs()[0].der);

    match scenario {
        "rotation" | "tls13" | "debounce" | "default" | "default12" | "observer" => {
            if scenario == "debounce" {
                fs::write(&cert, b"incomplete").unwrap();
            }
            fs::write(&cert, &pairs()[1].cert).unwrap();
            fs::write(&key, &pairs()[1].key).unwrap();
        }
        "rename" => {
            let cert_tmp = cert.with_extension("new");
            let key_tmp = key.with_extension("new");
            fs::write(&cert_tmp, &pairs()[1].cert).unwrap();
            fs::write(&key_tmp, &pairs()[1].key).unwrap();

            fs::rename(cert_tmp, &cert).unwrap();
            fs::rename(key_tmp, &key).unwrap();
        }
        "separate" => {
            fs::write(&cert, &pairs()[1].cert).unwrap();
            wait_for(&events, false);
            assert_eq!(served(&server, false), pairs()[0].der);
            fs::write(&key, &pairs()[1].key).unwrap();
        }
        "invalid" => {
            fs::write(&cert, b"incomplete PEM").unwrap();
            wait_for(&events, false);
            assert_eq!(served(&server, true), pairs()[0].der);

            // Wait for the bounded attempts to end, then repair via a new event.
            loop {
                if let Event::ReloadFailed { attempt: 6, .. } =
                    events.recv_timeout(Duration::from_secs(10)).unwrap()
                {
                    break;
                }
            }
            assert!(events.recv_timeout(Duration::from_millis(500)).is_err());
            fs::write(&cert, &pairs()[1].cert).unwrap();
            fs::write(&key, &pairs()[1].key).unwrap();
        }
        "shutdown" | "shutdown_pending" => {
            if scenario == "shutdown_pending" {
                fs::write(&cert, b"incomplete").unwrap();
                wait_for(&events, false);
            }

            let before = Instant::now();
            drop(watcher);
            assert!(before.elapsed() < Duration::from_secs(2));
            assert!(matches!(
                events.recv_timeout(Duration::from_secs(3)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ));
            fs::write(&cert, &pairs()[1].cert).unwrap();
            fs::write(&key, &pairs()[1].key).unwrap();
            assert_eq!(served(&server, false), pairs()[0].der);
            return;
        }
        _ => unreachable!(),
    }

    if scenario == "debounce" {
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(10)).unwrap(),
            Event::Reloaded
        ));
        assert!(events.recv_timeout(Duration::from_millis(300)).is_err());
    } else {
        wait_for(&events, true);
    }
    assert_eq!(served(&server, true), pairs()[1].der);
    assert_eq!(served(&server, false), pairs()[1].der);

    // A second rotation proves directory watches survive replacement of the original inode.
    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[0].key).unwrap();
    wait_for(&events, true);
    assert_eq!(served(&server, true), pairs()[0].der);

    drop(watcher);

    if scenario == "observer" {
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(3)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
}

#[test]
fn rotation() {
    exercise("rotation");
}

#[test]
fn debounce_in_shared_directory() {
    exercise("debounce");
}

#[test]
fn atomic_rename() {
    exercise("rename");
}

#[test]
fn separate_updates() {
    exercise("separate");
}

#[test]
fn invalid_replacement() {
    exercise("invalid");
}

#[test]
fn shutdown() {
    exercise("shutdown");
}

#[test]
fn shutdown_interrupts_retry() {
    exercise("shutdown_pending");
}

#[test]
fn tls13_rotation() {
    exercise("tls13");
}

#[test]
fn initial_validation() {
    validate_initial();
}

fn validate_initial() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");

    let load = || {
        hot_reload_rustls::Builder::new(&cert, &key)
            .crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
            .build()
            .map(|_| ())
    };

    let error = load().unwrap_err();
    assert!(
        matches!(&error, BuildError::Credentials(CredentialError::Io(source)) if source.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .is::<CredentialError>()
    );

    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[1].key).unwrap();
    assert!(
        matches!(
            load(),
            Err(BuildError::Credentials(CredentialError::Tls(_)))
        ),
        "mismatched initial pair must fail"
    );

    fs::write(&key, b"").unwrap();
    assert!(matches!(
        load(),
        Err(BuildError::Credentials(CredentialError::MissingPrivateKey))
    ));

    fs::write(&key, b"invalid private key").unwrap();
    assert!(load().is_err(), "invalid initial key must fail");

    fs::write(&key, &pairs()[0].key).unwrap();
    let mut invalid_chain = pairs()[0].cert.clone();
    invalid_chain
        .extend_from_slice(b"-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n");
    fs::write(&cert, invalid_chain).unwrap();
    assert!(load().is_err(), "invalid intermediate must fail");

    fs::write(&cert, &pairs()[0].cert).unwrap();
    assert!(load().is_ok(), "valid initial pair must succeed");
}

#[test]
fn default_builder_rotates_tls13() {
    exercise("default");
}

#[test]
fn default_builder_rotates_tls12() {
    exercise("default12");
}

#[test]
fn spawned_observer_reports_rotation() {
    exercise("observer");
}

#[test]
fn default_builder_rejects_tls11() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");

    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[0].key).unwrap();

    let (server, _watcher, _events) = start(&cert, &key, "default");

    // A TLS 1.1 ClientHello with TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256.
    // Rustls clients cannot offer obsolete protocol versions.
    let mut hello = vec![3, 2];
    hello.extend_from_slice(&[0; 32]);
    hello.extend_from_slice(&[0, 0, 2, 0xc0, 0x2f, 1, 0]);
    // Include signature algorithms so rejection reaches version negotiation.
    hello.extend_from_slice(&[0, 8, 0, 13, 0, 4, 0, 2, 4, 3]);
    let mut record = vec![
        22,
        3,
        2,
        0,
        (hello.len() + 4) as u8,
        1,
        0,
        0,
        hello.len() as u8,
    ];
    record.extend_from_slice(&hello);

    let mut tcp = TcpStream::connect(server.addr).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tcp.write_all(&record).unwrap();

    let mut alert = [0; 7];
    tcp.read_exact(&mut alert).unwrap();
    assert_eq!(alert[0], 21, "expected TLS alert");
    assert_eq!(
        &alert[5..],
        &[2, 70],
        "expected fatal protocol_version alert"
    );
}

#[test]
fn configuration_failure_preserves_cause() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[0].key).unwrap();

    let error = hot_reload_rustls::Builder::new(cert, key)
        .crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .configure(|_| {
            Err(BuildError::Configuration(
                std::io::Error::other("custom policy rejected").into(),
            ))
        })
        .build()
        .unwrap_err();

    assert!(matches!(error, BuildError::Configuration(_)));
    let cause = std::error::Error::source(&error).unwrap();
    assert_eq!(cause.to_string(), "custom policy rejected");
    assert_eq!(error.to_string(), "TLS configuration callback failed");
}
