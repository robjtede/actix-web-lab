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
use hot_reload_rustls::{Event, Watcher};
use openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    hash::MessageDigest,
    pkey::PKey,
    rsa::Rsa,
    ssl::{SslConnector, SslMethod, SslVerifyMode},
    x509::{X509, X509NameBuilder},
};

#[derive(Debug)]
struct WorkerKeyProvider;

impl rustls::crypto::KeyProvider for WorkerKeyProvider {
    fn load_private_key(
        &self,
        key: rustls::pki_types::PrivateKeyDer<'static>,
    ) -> Result<Arc<dyn rustls::sign::SigningKey>, rustls::Error> {
        assert_eq!(thread::current().name(), Some(env!("CARGO_PKG_NAME")));
        rustls::crypto::ring::default_provider()
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
            let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();

            let mut name = X509NameBuilder::new().unwrap();
            name.append_entry_by_text("CN", "localhost").unwrap();
            let name = name.build();

            let mut cert = X509::builder().unwrap();
            cert.set_version(2).unwrap();
            cert.set_serial_number(&BigNum::from_u32(serial).unwrap().to_asn1_integer().unwrap())
                .unwrap();
            cert.set_subject_name(&name).unwrap();
            cert.set_issuer_name(&name).unwrap();
            cert.set_pubkey(&key).unwrap();
            cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
                .unwrap();
            cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
                .unwrap();

            cert.sign(&key, MessageDigest::sha256()).unwrap();
            let cert = cert.build();

            Pair {
                cert: cert.to_pem().unwrap(),
                key: key.private_key_to_pem_pkcs8().unwrap(),
                der: cert.to_der().unwrap(),
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
        let mut provider = rustls::crypto::ring::default_provider();
        provider.key_provider = &WorkerKeyProvider;
        let provider = Arc::new(provider);
        let builder = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[if tls13 {
                &rustls::version::TLS13
            } else {
                &rustls::version::TLS12
            }])
            .unwrap()
            .with_no_client_auth();
        let (config, mut watcher) = if defaults {
            hot_reload_rustls::Builder::new(cert, key).build().unwrap()
        } else {
            hot_reload_rustls::Builder::new(cert, key)
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
    // A fresh connector per probe prevents session resumption.
    let mut connector = SslConnector::builder(SslMethod::tls_client()).unwrap();
    connector.set_verify(SslVerifyMode::NONE);
    if server.defaults {
        connector
            .set_max_proto_version(Some(if server.tls13 {
                openssl::ssl::SslVersion::TLS1_3
            } else {
                openssl::ssl::SslVersion::TLS1_2
            }))
            .unwrap();
    }
    connector.set_alpn_protos(b"\x08http/1.1").unwrap();

    let mut config = connector.build().configure().unwrap();
    config.set_use_server_name_indication(sni);

    let tcp = TcpStream::connect_timeout(&server.addr, Duration::from_secs(3)).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();

    let mut tls = config.connect("localhost", tcp).unwrap();
    assert_eq!(
        tls.ssl().version_str(),
        if server.tls13 { "TLSv1.3" } else { "TLSv1.2" }
    );
    if !server.defaults {
        assert_eq!(tls.ssl().selected_alpn_protocol(), Some(&b"http/1.1"[..]));
    }

    let der = tls.ssl().peer_certificate().unwrap().to_der().unwrap();

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
            .tls_config(
                rustls::ServerConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth(),
            )
            .build()
            .map(|_| ())
    };

    assert!(load().is_err(), "missing initial files must fail");

    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[1].key).unwrap();
    assert!(load().is_err(), "mismatched initial pair must fail");

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

#[cfg(feature = "ring")]
#[test]
fn default_builder_rotates_tls13() {
    exercise("default");
}

#[cfg(feature = "ring")]
#[test]
fn default_builder_rotates_tls12() {
    exercise("default12");
}

#[test]
fn spawned_observer_reports_rotation() {
    exercise("observer");
}

#[cfg(feature = "ring")]
#[test]
fn default_builder_rejects_tls11() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");

    fs::write(&cert, &pairs()[0].cert).unwrap();
    fs::write(&key, &pairs()[0].key).unwrap();

    let (server, _watcher, _events) = start(&cert, &key, "default");

    let mut connector = SslConnector::builder(SslMethod::tls_client()).unwrap();
    connector.set_verify(SslVerifyMode::NONE);
    connector.set_security_level(0);
    connector.set_cipher_list("ALL:@SECLEVEL=0").unwrap();
    connector
        .set_max_proto_version(Some(openssl::ssl::SslVersion::TLS1_1))
        .unwrap();

    let tcp = TcpStream::connect(server.addr).unwrap();
    tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();

    let error = connector.build().connect("localhost", tcp).unwrap_err();
    assert!(error.to_string().contains("SSL alert number"), "{error}");
}
